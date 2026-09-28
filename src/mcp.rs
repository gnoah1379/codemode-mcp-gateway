use crate::{execution, registry, state::GatewayState};
use anyhow::Result;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
        Implementation, ListToolsResult, ServerCapabilities, ServerConfig, Tool,
    },
    service::{NotificationContext, RequestContext, RoleServer},
    transport::stdio,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{borrow::Cow, sync::Arc};

#[derive(Clone)]
pub struct McpGateway {
    pub state: Arc<GatewayState>,
}
impl McpGateway {
    pub fn new(state: Arc<GatewayState>) -> Self {
        Self { state }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    query: String,
    namespaces: Option<Vec<String>>,
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteInput {
    code: String,
    timeout_ms: Option<u64>,
}

impl ServerHandler for McpGateway {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_tool_list_changed().build())
            .with_server_info(Implementation::new("code-mode-mcp-server", env!("CARGO_PKG_VERSION")))
            .with_instructions("Discover upstream tools with tools_search, then execute JavaScript with tools_execute.")
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let snapshot = self.state.snapshot().await;
        let search = make_tool(
            "tools_search",
            snapshot.catalog_description.clone(),
            search_schema(),
        );
        let execute = make_tool(
            "tools_execute",
            execution::description().to_owned(),
            execute_schema(),
        );
        // Spec 2026-07-28 requires cache hints; the catalog description changes with upstreams
        // and the endpoint is per-credential, so clients must revalidate and must not share it.
        Ok(ListToolsResult::with_all_items(vec![search, execute])
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let (value, is_error) = match request.name.as_ref() {
            "tools_search" => match serde_json::from_value::<SearchInput>(args) {
                Ok(input) => match self.search(input).await {
                    Ok(value) => (value, false),
                    Err((code, message)) => (error_value(code, message), true),
                },
                Err(_) => (
                    error_value("INVALID_INPUT", "Invalid tools_search input."),
                    true,
                ),
            },
            "tools_execute" => match serde_json::from_value::<ExecuteInput>(args) {
                Ok(input) => {
                    let value = self
                        .state
                        .executions
                        .run(
                            self.state.clone(),
                            input.code,
                            input.timeout_ms,
                            Some(context.ct.clone()),
                        )
                        .await;
                    let is_error = value.get("error").is_some();
                    (value, is_error)
                }
                Err(_) => (
                    error_value("INVALID_INPUT", "Invalid tools_execute input."),
                    true,
                ),
            },
            _ => {
                return Err(rmcp::ErrorData::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >());
            }
        };
        Ok(CallToolResponse::Complete(text_result(value, is_error)))
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let mut peers = self.state.peers.lock().await;
        peers.push(context.peer.clone());
    }
}

impl McpGateway {
    async fn search(
        &self,
        input: SearchInput,
    ) -> std::result::Result<Value, (&'static str, &'static str)> {
        let query = input.query.trim();
        if query.is_empty() {
            return Err(("INVALID_QUERY", "Search query must not be empty."));
        }
        if query.len() > 4096 {
            return Err(("QUERY_TOO_LARGE", "Search query exceeds 4 KiB."));
        }
        let first = self.state.snapshot().await;
        let limit = input.limit.unwrap_or(first.config.search.default_limit);
        if limit == 0 || limit > first.config.search.max_limit {
            return Err((
                "INVALID_LIMIT",
                "Search limit is outside the configured range.",
            ));
        }
        let namespaces = input.namespaces.unwrap_or_default();
        let ids = first.search.search(query, &namespaces, limit);
        let current = self.state.snapshot().await;
        let (snapshot, ids) = if current.generation == first.generation {
            (current, ids)
        } else {
            let retry_limit = limit.min(current.config.search.max_limit);
            let retry = current.search.search(query, &namespaces, retry_limit);
            (current, retry)
        };
        let allowed: std::collections::HashSet<String> =
            registry::filtered_tools(&snapshot.registry, &snapshot.config.policy)
                .into_iter()
                .map(|tool| tool.full_name())
                .collect();
        let items: Vec<_> = ids
            .into_iter()
            .filter_map(|id| {
                snapshot
                    .registry
                    .tools
                    .get(&id)
                    .filter(|_| allowed.contains(&id))
                    .map(|tool| tool.public())
            })
            .collect();
        let byte_limit = snapshot.config.search.max_response_bytes;
        let mut selected = Vec::new();
        if items.is_empty() {
            return Ok(json!([]));
        }
        for item in items {
            selected.push(item);
            let candidate = serde_json::to_value(&selected)
                .map_err(|_| ("INTERNAL_ERROR", "Search serialization failed."))?;
            let response = text_result(candidate, false);
            let bytes = serde_json::to_vec(&response)
                .map(|v| v.len() + 128)
                .unwrap_or(usize::MAX);
            if bytes > byte_limit {
                selected.pop();
                if selected.is_empty() {
                    return Err((
                        "SEARCH_RESULT_TOO_LARGE",
                        "The first matching tool schema exceeds the response limit.",
                    ));
                }
                break;
            }
        }
        serde_json::to_value(selected)
            .map_err(|_| ("INTERNAL_ERROR", "Search serialization failed."))
    }
}

pub async fn serve_stdio(state: Arc<GatewayState>) -> Result<()> {
    let server = McpGateway::new(state).serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

pub fn search_schema() -> Map<String, Value> {
    json!({"type":"object","properties":{"query":{"type":"string","minLength":1},"namespaces":{"type":"array","items":{"type":"string"}},"limit":{"type":"integer","minimum":1}},"required":["query"],"additionalProperties":false}).as_object().cloned().unwrap_or_default()
}
pub fn execute_schema() -> Map<String, Value> {
    json!({"type":"object","properties":{"code":{"type":"string","maxLength":65536},"timeout_ms":{"type":"integer","minimum":1}},"required":["code"],"additionalProperties":false}).as_object().cloned().unwrap_or_default()
}
fn make_tool(name: &str, description: String, schema: Map<String, Value>) -> Tool {
    let mut tool = Tool::default();
    tool.name = Cow::Owned(name.to_owned());
    tool.description = Some(Cow::Owned(description));
    tool.input_schema = Arc::new(schema);
    tool
}
fn error_value(code: &str, message: &str) -> Value {
    json!({"error":{"code":code,"message":message}})
}
fn text_result(value: Value, is_error: bool) -> CallToolResult {
    let serialized = serde_json::to_string(&value).unwrap_or_else(|_| "{\"error\":{\"code\":\"RESULT_NOT_SERIALIZABLE\",\"message\":\"Result serialization failed.\"}}".into());
    if is_error {
        CallToolResult::error(vec![ContentBlock::text(serialized)])
    } else {
        CallToolResult::success(vec![ContentBlock::text(serialized)])
    }
}
