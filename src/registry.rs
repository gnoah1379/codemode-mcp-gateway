use crate::{
    config::{Config, UpstreamConfig, UpstreamTransport},
    policy,
};
use anyhow::{Context, Result, anyhow};
use futures::{SinkExt, StreamExt};
use http::{HeaderName, HeaderValue};
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult, PaginatedRequestParams},
    service::{RoleClient, RunningService, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{
        StreamableHttpClientTransport, Transport, async_rw::JsonRpcMessageCodec,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    io,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    process::{ChildStdin, ChildStdout, Command},
    sync::{Mutex, Semaphore},
};
use tokio_util::codec::{FramedRead, FramedWrite};

pub type UpstreamClient = RunningService<RoleClient, ()>;
const MAX_UPSTREAM_STDIO_FRAME_BYTES: usize = 8 * 1024 * 1024;
type ChildTransportWriter =
    Arc<Mutex<Option<FramedWrite<ChildStdin, JsonRpcMessageCodec<TxJsonRpcMessage<RoleClient>>>>>>;

struct BoundedChildTransport {
    child: Option<Box<dyn ChildWrapper>>,
    reader: FramedRead<ChildStdout, JsonRpcMessageCodec<RxJsonRpcMessage<RoleClient>>>,
    writer: ChildTransportWriter,
}

impl BoundedChildTransport {
    fn spawn(command: Command) -> io::Result<Self> {
        let mut wrapped = CommandWrap::from(command);
        #[cfg(unix)]
        wrapped.wrap(ProcessGroup::leader());
        let mut child = wrapped.spawn()?;
        let stdin = child
            .stdin()
            .take()
            .ok_or_else(|| io::Error::other("upstream stdin unavailable"))?;
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| io::Error::other("upstream stdout unavailable"))?;
        Ok(Self {
            child: Some(child),
            reader: FramedRead::new(
                stdout,
                JsonRpcMessageCodec::new_with_max_length(MAX_UPSTREAM_STDIO_FRAME_BYTES),
            ),
            writer: Arc::new(Mutex::new(Some(FramedWrite::new(
                stdin,
                JsonRpcMessageCodec::new(),
            )))),
        })
    }
}

impl Transport<RoleClient> for BoundedChildTransport {
    type Error = io::Error;
    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let writer = self.writer.clone();
        async move {
            let mut writer = writer.lock().await;
            let Some(writer) = writer.as_mut() else {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "upstream transport closed",
                ));
            };
            writer.send(message).await.map_err(io::Error::other)
        }
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        match self.reader.next().await {
            Some(Ok(message)) => Some(message),
            Some(Err(_)) | None => None,
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        self.writer.lock().await.take();
        if let Some(mut child) = self.child.take() {
            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(result) => {
                    result?;
                }
                Err(_) => {
                    Box::into_pin(child.kill()).await?;
                }
            }
        }
        Ok(())
    }
}

impl Drop for BoundedChildTransport {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = Box::into_pin(child.kill()).await;
                });
            } else {
                let _ = child.start_kill();
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolRecord {
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub input_validator: Arc<jsonschema::Validator>,
    pub output_schema: Option<Value>,
    pub generation: u64,
}
impl ToolRecord {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
    pub fn public(&self) -> PublicTool {
        PublicTool {
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicTool {
    pub namespace: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "outputSchema")]
    pub output_schema: Option<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct UpstreamStatus {
    pub name: String,
    pub description: String,
    pub transport: String,
    pub enabled: bool,
    pub available: bool,
    pub tool_count: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Registry {
    pub clients: BTreeMap<String, Arc<UpstreamClient>>,
    pub tools: BTreeMap<String, ToolRecord>,
    pub upstreams: BTreeMap<String, UpstreamStatus>,
    pub upstream_limits: BTreeMap<String, Arc<Semaphore>>,
}

pub async fn prepare(config: &Config, generation: u64) -> Registry {
    let mut registry = Registry::default();
    for (name, upstream) in &config.upstreams {
        let description = crate::config::normalize_description(&upstream.description);
        let transport_name = match &upstream.transport {
            UpstreamTransport::Stdio { .. } => "stdio",
            UpstreamTransport::StreamableHttp { .. } => "streamable_http",
        }
        .to_owned();
        if upstream.enabled {
            registry.upstream_limits.insert(
                name.clone(),
                Arc::new(Semaphore::new(config.sandbox.max_parallel_tool_calls)),
            );
        }
        if !upstream.enabled {
            registry.upstreams.insert(
                name.clone(),
                UpstreamStatus {
                    name: name.clone(),
                    description,
                    transport: transport_name,
                    enabled: false,
                    available: false,
                    tool_count: 0,
                    last_error: None,
                },
            );
            continue;
        }
        match tokio::time::timeout(
            Duration::from_secs(10),
            discover(name, upstream, generation),
        )
        .await
        {
            Ok(Ok((client, tools))) => {
                let count = tools.len();
                registry.clients.insert(name.clone(), Arc::new(client));
                for mut tool in tools {
                    tool.namespace = name.clone();
                    registry.tools.insert(tool.full_name(), tool);
                }
                registry.upstreams.insert(
                    name.clone(),
                    UpstreamStatus {
                        name: name.clone(),
                        description,
                        transport: transport_name,
                        enabled: true,
                        available: true,
                        tool_count: count,
                        last_error: None,
                    },
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(upstream = %name, "upstream discovery failed; marked degraded");
                let _ = error;
                registry.upstreams.insert(
                    name.clone(),
                    UpstreamStatus {
                        name: name.clone(),
                        description,
                        transport: transport_name,
                        enabled: true,
                        available: false,
                        tool_count: 0,
                        last_error: Some("upstream connection or discovery failed".into()),
                    },
                );
            }
            Err(_) => {
                registry.upstreams.insert(
                    name.clone(),
                    UpstreamStatus {
                        name: name.clone(),
                        description,
                        transport: transport_name,
                        enabled: true,
                        available: false,
                        tool_count: 0,
                        last_error: Some("upstream connection timed out".into()),
                    },
                );
            }
        }
    }
    registry
}

async fn discover(
    name: &str,
    config: &UpstreamConfig,
    generation: u64,
) -> Result<(UpstreamClient, Vec<ToolRecord>)> {
    let client = connect(&config.transport).await?;
    let mut all = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut seen_cursors = std::collections::HashSet::new();
    let mut total_schema_bytes = 0usize;
    let mut cursor: Option<String> = None;
    loop {
        let params = cursor
            .clone()
            .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor)));
        let result = client
            .list_tools(params)
            .await
            .context("tools/list failed")?;
        for tool in result.tools {
            if all.len() >= 10_000 {
                return bail_upstream("upstream tool list exceeds 10,000 tools");
            }
            let input_schema = serde_json::to_value(&*tool.input_schema)?;
            let output_schema = tool
                .output_schema
                .as_ref()
                .map(|schema| serde_json::to_value(&**schema))
                .transpose()?;
            let input_bytes = serde_json::to_vec(&input_schema)?.len();
            let output_bytes = output_schema
                .as_ref()
                .map(|v| serde_json::to_vec(v).map(|b| b.len()).unwrap_or(usize::MAX))
                .unwrap_or(0);
            if input_bytes > 512 * 1024 || output_bytes > 512 * 1024 {
                return bail_upstream("upstream tool schema exceeds 512 KiB");
            }
            total_schema_bytes = total_schema_bytes
                .saturating_add(input_bytes)
                .saturating_add(output_bytes);
            if total_schema_bytes > 64 * 1024 * 1024 {
                return bail_upstream("upstream catalog schemas exceed 64 MiB");
            }
            if schema_depth(&input_schema) > 32
                || output_schema
                    .as_ref()
                    .is_some_and(|schema| schema_depth(schema) > 32)
            {
                return bail_upstream("upstream tool schema exceeds depth 32");
            }
            if has_external_schema_ref(&input_schema) {
                return bail_upstream("input schema contains a non-local reference");
            }
            let input_validator = jsonschema::validator_for(&input_schema)
                .context("upstream input schema is not valid JSON Schema")?;
            let name = tool.name.to_string();
            if name.is_empty() || name.len() > 256 || name.contains('\0') {
                continue;
            }
            if !seen.insert(name.clone()) {
                return bail_upstream("upstream returned duplicate tool names");
            }
            all.push(ToolRecord {
                namespace: String::new(),
                name,
                description: tool.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema,
                input_validator: Arc::new(input_validator),
                output_schema,
                generation,
            });
        }
        cursor = result.next_cursor;
        if cursor
            .as_ref()
            .is_some_and(|next| !seen_cursors.insert(next.clone()))
        {
            return bail_upstream("upstream repeated a pagination cursor");
        }
        if cursor.is_none() {
            break;
        }
        if all.len() >= 10_000 {
            return bail_upstream("upstream pagination exceeded tool limit");
        }
    }
    let _ = name;
    // The namespace comes from config and is set after preserving original tool names.
    // A duplicate original mapping is rejected before catalog publication.
    Ok((client, all))
}

fn bail_upstream<T>(message: &str) -> Result<T> {
    Err(anyhow!(message.to_owned()))
}
fn schema_depth(value: &Value) -> usize {
    match value {
        Value::Array(a) => 1 + a.iter().map(schema_depth).max().unwrap_or(0),
        Value::Object(o) => 1 + o.values().map(schema_depth).max().unwrap_or(0),
        _ => 0,
    }
}

async fn connect(transport: &UpstreamTransport) -> Result<UpstreamClient> {
    match transport {
        UpstreamTransport::Stdio { command, args, env } => {
            let mut process = Command::new(command);
            process
                .args(args)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            for (key, reference) in env {
                process.env(
                    key,
                    std::env::var(&reference.from_env).with_context(|| {
                        format!("missing environment variable {}", reference.from_env)
                    })?,
                );
            }
            let transport =
                BoundedChildTransport::spawn(process).context("failed to start stdio upstream")?;
            ().serve(transport)
                .await
                .context("stdio upstream MCP initialize failed")
        }
        UpstreamTransport::StreamableHttp { url, headers } => {
            let mut resolved = HashMap::new();
            for (name, reference) in headers {
                let name =
                    HeaderName::from_bytes(name.as_bytes()).context("invalid HTTP header name")?;
                let value =
                    HeaderValue::from_str(&std::env::var(&reference.from_env).with_context(
                        || format!("missing environment variable {}", reference.from_env),
                    )?)
                    .context("invalid HTTP header value")?;
                resolved.insert(name, value);
            }
            let cfg = StreamableHttpClientTransportConfig::with_uri(url.as_str())
                .custom_headers(resolved)
                .max_sse_event_size(8 * 1024 * 1024)
                .max_concurrent_requests(4);
            let transport = StreamableHttpClientTransport::from_config(cfg);
            ().serve(transport)
                .await
                .context("HTTP upstream MCP initialize failed")
        }
    }
}

pub async fn call(
    client: &UpstreamClient,
    tool: &ToolRecord,
    args: Value,
) -> Result<CallToolResult> {
    if !args.is_object() || !tool.input_validator.is_valid(&args) {
        return Err(anyhow!("INVALID_ARGUMENTS:arguments do not match schema"));
    }
    let object: Map<String, Value> = args
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("INVALID_ARGUMENTS:arguments must be a JSON object"))?;
    let result = client
        .call_tool(CallToolRequestParams::new(tool.name.clone()).with_arguments(object))
        .await
        .context("UPSTREAM_TRANSPORT_ERROR")?;
    let size = serde_json::to_vec(&result)
        .map(|b| b.len())
        .unwrap_or(usize::MAX);
    if size > 2 * 1024 * 1024 {
        return Err(anyhow!("UPSTREAM_RESULT_TOO_LARGE"));
    }
    Ok(result)
}

pub fn filtered_tools<'a>(
    registry: &'a Registry,
    policy_config: &crate::config::PolicyConfig,
) -> Vec<&'a ToolRecord> {
    registry
        .tools
        .values()
        .filter(|tool| {
            registry
                .upstreams
                .get(&tool.namespace)
                .is_some_and(|status| status.enabled && status.available)
                && policy::evaluate(policy_config, &tool.full_name()).allowed
        })
        .collect()
}

fn has_external_schema_ref(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef")
                && value
                    .as_str()
                    .is_some_and(|reference| !reference.starts_with('#')))
                || has_external_schema_ref(value)
        }),
        Value::Array(items) => items.iter().any(has_external_schema_ref),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Registry, ToolRecord, UpstreamStatus, filtered_tools};
    use crate::config::PolicyConfig;
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Arc};

    fn tool(namespace: &str, name: &str) -> ToolRecord {
        let schema = json!({"type":"object"});
        ToolRecord {
            namespace: namespace.into(),
            name: name.into(),
            description: name.into(),
            input_schema: schema.clone(),
            input_validator: Arc::new(jsonschema::validator_for(&schema).unwrap()),
            output_schema: None,
            generation: 1,
        }
    }

    #[test]
    fn denied_offline_and_disabled_tools_are_excluded_from_catalog() {
        let tools = [
            tool("github", "list_issues"),
            tool("github", "delete_issue"),
            tool("offline", "search"),
            tool("disabled", "search"),
        ]
        .into_iter()
        .map(|record| (record.full_name(), record))
        .collect::<BTreeMap<_, _>>();
        let upstreams = [
            ("github", true, true, None),
            ("offline", true, false, Some("unavailable")),
            ("disabled", false, false, None),
        ]
        .into_iter()
        .map(|(name, enabled, available, error)| {
            (
                name.into(),
                UpstreamStatus {
                    name: name.into(),
                    description: name.into(),
                    transport: "stdio".into(),
                    enabled,
                    available,
                    tool_count: 1,
                    last_error: error.map(str::to_owned),
                },
            )
        })
        .collect();
        let registry = Registry {
            tools,
            upstreams,
            ..Registry::default()
        };
        let policy = PolicyConfig {
            default: "allow".into(),
            allow: vec![],
            deny: vec!["*.delete_*".into()],
        };

        let visible = filtered_tools(&registry, &policy);
        let names: Vec<_> = visible.iter().map(|record| record.full_name()).collect();
        assert_eq!(names, vec!["github.list_issues"]);
    }
}
