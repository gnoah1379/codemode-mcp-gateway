use crate::{
    assets, config,
    mcp::McpGateway,
    policy, registry,
    state::{GatewayEvent, GatewayState},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures::Stream;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};

pub async fn serve(state: Arc<GatewayState>) -> anyhow::Result<()> {
    let snapshot = state.snapshot().await;
    let address: SocketAddr = snapshot.config.server.listen.parse()?;
    let mcp_path = snapshot.config.server.mcp_path.clone();
    let mut allowed_hosts = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "[::1]".to_owned(),
        address.to_string(),
    ];
    if let Ok(extra) = std::env::var("GATEWAY_ALLOWED_HOSTS") {
        allowed_hosts.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    let mut allowed_origins = vec![
        format!("http://localhost:{}", address.port()),
        format!("http://127.0.0.1:{}", address.port()),
    ];
    if let Ok(extra) = std::env::var("GATEWAY_ALLOWED_ORIGINS") {
        allowed_origins.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    let mut http_config = StreamableHttpServerConfig::default();
    http_config.allowed_hosts = allowed_hosts;
    http_config.allowed_origins = allowed_origins;
    http_config.max_request_body_bytes = 256 * 1024;
    http_config.json_response = true;
    let service_state = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(McpGateway::new(service_state.clone())),
        Arc::new(LocalSessionManager::default()),
        http_config,
    );

    let refresh_state = state.clone();
    tokio::spawn(async move {
        let mut retry_delay = 300u64;
        loop {
            let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]) % 30;
            tokio::time::sleep(std::time::Duration::from_secs(retry_delay + jitter)).await;
            match refresh_state.refresh_upstreams().await {
                Ok(true) => retry_delay = (retry_delay.saturating_mul(2)).min(3600),
                Ok(false) => retry_delay = 300,
                Err(error) => {
                    tracing::warn!(error = %error, "periodic upstream refresh failed; retaining the current snapshot");
                    retry_delay = (retry_delay.saturating_mul(2)).min(3600);
                }
            }
        }
    });

    let protected: Router = Router::new()
        .route("/config", get(get_config).put(put_config))
        .route("/config/validate", post(validate_config))
        .route("/config/reload", post(reload_config))
        .route("/upstreams", get(get_upstreams))
        .route("/catalog-preview", get(catalog_preview))
        .route("/metrics", get(get_metrics))
        .route("/upstreams/{name}/test", post(test_upstream))
        .route("/upstreams/{name}/refresh", post(refresh_upstream))
        .route("/tools", get(get_tools))
        .route("/policy/evaluate", post(evaluate_policy))
        .route("/executions", get(get_executions))
        .route("/executions/{id}", get(get_execution))
        .route("/executions/{id}/cancel", post(cancel_execution))
        .route("/events", get(events))
        .layer(middleware::from_fn_with_state(state.clone(), admin_auth))
        .with_state(state.clone());
    let sessions: Router = Router::new()
        .route("/session", post(create_session))
        .route("/session", axum::routing::delete(delete_session))
        .with_state(state.clone());
    let mcp_path_static = mcp_path.clone();
    let ready_state = state.clone();
    let app: Router = Router::new()
        .nest("/api/v1", protected.merge(sessions))
        .route("/health/live", get(|| async { Json(json!({"status":"live"})) }))
        .route("/health/ready", get(move || { let state = ready_state.clone(); async move { let snapshot = state.snapshot().await; Json(json!({"status":"ready","degradedUpstreams":snapshot.registry.upstreams.values().filter(|u|u.enabled&&!u.available).count()})) } }))
        .route_service(&mcp_path_static, service)
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(state.clone(), host_origin_and_client_auth))
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, %mcp_path, "gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn get_config(State(state): State<Arc<GatewayState>>) -> Response {
    let snapshot = state.snapshot().await;
    match serde_yaml::to_string(&*snapshot.config) {
        Ok(yaml) => (
            [
                (header::CONTENT_TYPE, "application/yaml; charset=utf-8"),
                (header::ETAG, snapshot.revision.as_str()),
                (header::CACHE_CONTROL, "no-store"),
            ],
            yaml,
        )
            .into_response(),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CONFIG_SERIALIZATION_FAILED",
            "Could not serialize the current configuration.",
        ),
    }
}

async fn put_config(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() > 1024 * 1024 {
        return api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "CONFIG_TOO_LARGE",
            "Configuration exceeds 1 MiB.",
        );
    }
    let yaml = match std::str::from_utf8(&body) {
        Ok(value) => value,
        Err(_) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "INVALID_CONFIG",
                "Configuration must be UTF-8 YAML.",
            );
        }
    };
    let candidate = match config::parse(yaml) {
        Ok(candidate) => candidate,
        Err(error) => {
            tracing::warn!(error = %error, "configuration rejected");
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_CONFIG",
                "Configuration is invalid.",
            );
        }
    };
    let expected = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok());
    if expected.is_none() {
        return api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "REVISION_REQUIRED",
            "Read the current configuration and send its ETag in If-Match.",
        );
    }
    match state.replace_config(candidate, expected, true).await {
        Ok(revision) => (
            StatusCode::OK,
            [(header::ETAG, revision.as_str())],
            Json(json!({"ok":true,"revision":revision})),
        )
            .into_response(),
        Err(error) if error.to_string().contains("REVISION_CONFLICT") => api_error(
            StatusCode::CONFLICT,
            "REVISION_CONFLICT",
            "Configuration changed since it was read; reload before saving.",
        ),
        Err(error) => {
            tracing::error!(error = %error, "configuration commit failed");
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "CONFIG_COMMIT_FAILED",
                "Configuration could not be prepared or saved.",
            )
        }
    }
}

async fn validate_config(State(_state): State<Arc<GatewayState>>, body: Bytes) -> Response {
    let Ok(yaml) = std::str::from_utf8(&body) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_CONFIG",
            "Configuration must be UTF-8 YAML.",
        );
    };
    match config::parse(yaml) {
        Ok(candidate) => {
            let registry = registry::prepare(&candidate, 0).await;
            (StatusCode::OK, Json(json!({"valid":true,"degradedUpstreams":registry.upstreams.values().filter(|upstream| upstream.enabled && !upstream.available).map(|upstream| upstream.name.clone()).collect::<Vec<_>>() }))).into_response()
        }
        Err(error) => {
            tracing::debug!(error = %error, "configuration validation failed");
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_CONFIG",
                "Configuration is invalid.",
            )
        }
    }
}

async fn reload_config(State(state): State<Arc<GatewayState>>) -> Response {
    match state.reload_file().await {
        Ok(revision) => Json(json!({"ok":true,"revision":revision})).into_response(),
        Err(error) => {
            tracing::warn!(error = %error, "config reload rejected; keeping active snapshot");
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "RELOAD_FAILED",
                "Configuration reload failed; the running configuration remains active.",
            )
        }
    }
}

async fn get_upstreams(State(state): State<Arc<GatewayState>>) -> Json<Value> {
    Json(json!({"upstreams":state.upstream_status().await}))
}
async fn catalog_preview(State(state): State<Arc<GatewayState>>) -> Json<Value> {
    Json(json!({"description":state.snapshot().await.catalog_description}))
}
async fn get_metrics(State(state): State<Arc<GatewayState>>) -> Response {
    let snapshot = state.snapshot().await;
    let discovered_tools = snapshot.registry.tools.len();
    let available_tools =
        registry::filtered_tools(&snapshot.registry, &snapshot.config.policy).len();
    let enabled_upstreams = snapshot
        .registry
        .upstreams
        .values()
        .filter(|upstream| upstream.enabled)
        .count();
    let healthy_upstreams = snapshot
        .registry
        .upstreams
        .values()
        .filter(|upstream| upstream.enabled && upstream.available)
        .count();
    let db = state.db.lock().await;
    let result = db.query_row(
        "SELECT COUNT(*),SUM(outcome='succeeded'),SUM(outcome='failed'),SUM(outcome='timed_out'),SUM(outcome='cancelled'),SUM(outcome IN ('queued','running')),COALESCE(AVG(duration_ms),0),COALESCE(SUM(output_bytes),0),(SELECT COUNT(*) FROM execution_events WHERE kind='tool_call') FROM executions",
        [],
        |row| Ok((row.get::<_, i64>(0)?,row.get::<_, Option<i64>>(1)?.unwrap_or_default(),row.get::<_, Option<i64>>(2)?.unwrap_or_default(),row.get::<_, Option<i64>>(3)?.unwrap_or_default(),row.get::<_, Option<i64>>(4)?.unwrap_or_default(),row.get::<_, Option<i64>>(5)?.unwrap_or_default(),row.get::<_, f64>(6)?,row.get::<_, i64>(7)?,row.get::<_, i64>(8)?)),
    );
    match result {
        Ok((total,succeeded,failed,timed_out,cancelled,active,average_duration_ms,output_bytes,tool_calls)) => Json(json!({"executions":{"total":total,"succeeded":succeeded,"failed":failed,"timedOut":timed_out,"cancelled":cancelled,"active":active,"averageDurationMs":average_duration_ms,"outputBytes":output_bytes},"toolCalls":tool_calls,"tools":{"discovered":discovered_tools,"available":available_tools},"upstreams":{"total":enabled_upstreams,"healthy":healthy_upstreams}})).into_response(),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR,"METRICS_UNAVAILABLE","Execution metrics could not be read."),
    }
}
async fn test_upstream(
    State(state): State<Arc<GatewayState>>,
    Path(name): Path<String>,
) -> Response {
    let snapshot = state.snapshot().await;
    if !snapshot.config.upstreams.contains_key(&name) {
        return api_error(
            StatusCode::NOT_FOUND,
            "UPSTREAM_NOT_FOUND",
            "Upstream was not found.",
        );
    }
    let prepared = registry::prepare(&snapshot.config, snapshot.generation + 1).await;
    match prepared.upstreams.get(&name) {
        Some(status) => (StatusCode::OK, Json(json!({"name":name,"available":status.available,"toolCount":status.tool_count,"error":status.last_error}))).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "UPSTREAM_NOT_FOUND", "Upstream was not found."),
    }
}
async fn refresh_upstream(
    State(state): State<Arc<GatewayState>>,
    Path(name): Path<String>,
) -> Response {
    let snapshot = state.snapshot().await;
    if !snapshot.config.upstreams.contains_key(&name) {
        return api_error(
            StatusCode::NOT_FOUND,
            "UPSTREAM_NOT_FOUND",
            "Upstream was not found.",
        );
    }
    match state
        .replace_config((*snapshot.config).clone(), Some(&snapshot.revision), false)
        .await
    {
        Ok(revision) => (
            StatusCode::OK,
            Json(json!({"ok":true,"revision":revision,"upstream":name})),
        )
            .into_response(),
        Err(_) => api_error(
            StatusCode::CONFLICT,
            "REVISION_CONFLICT",
            "Configuration changed while refreshing the upstream.",
        ),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminTool {
    namespace: String,
    name: String,
    description: String,
    input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_schema: Option<Value>,
    available: bool,
    allowed: bool,
    matched_pattern: Option<String>,
}
async fn get_tools(
    State(state): State<Arc<GatewayState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let snapshot = state.snapshot().await;
    let namespace = query.get("namespace").map(String::as_str);
    let search = query.get("query").map(|q| q.to_lowercase());
    let page = query
        .get("page")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = query
        .get("page_size")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let mut tools: Vec<_> = snapshot
        .registry
        .tools
        .values()
        .filter(|tool| namespace.is_none_or(|n| n == tool.namespace))
        .filter(|tool| {
            search.as_ref().is_none_or(|q| {
                tool.full_name().to_lowercase().contains(q)
                    || tool.description.to_lowercase().contains(q)
            })
        })
        .map(|tool| {
            let decision = policy::evaluate(&snapshot.config.policy, &tool.full_name());
            let available = snapshot
                .registry
                .upstreams
                .get(&tool.namespace)
                .is_some_and(|upstream| upstream.available && upstream.enabled);
            AdminTool {
                namespace: tool.namespace.clone(),
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                output_schema: tool.output_schema.clone(),
                available,
                allowed: decision.allowed,
                matched_pattern: decision.matched_pattern,
            }
        })
        .collect();
    tools.sort_by(|a, b| {
        format!("{}.{}", a.namespace, a.name).cmp(&format!("{}.{}", b.namespace, b.name))
    });
    let total = tools.len();
    let start = (page - 1).saturating_mul(page_size);
    let selected: Vec<_> = tools
        .drain(start.min(total)..((start + page_size).min(total)))
        .collect();
    Json(json!({"items":selected,"page":page,"pageSize":page_size,"total":total}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyRequest {
    name: String,
}
async fn evaluate_policy(
    State(state): State<Arc<GatewayState>>,
    Json(input): Json<PolicyRequest>,
) -> Response {
    if input.name.split_once('.').is_none() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_TOOL_NAME",
            "Tool name must use namespace.name format.",
        );
    }
    let snapshot = state.snapshot().await;
    let decision = policy::evaluate(&snapshot.config.policy, &input.name);
    Json(json!({"name":input.name,"allowed":decision.allowed,"matchedPattern":decision.matched_pattern,"default":snapshot.config.policy.default})).into_response()
}

async fn get_executions(
    State(state): State<Arc<GatewayState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let limit = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    Json(json!({"items":state.executions.list(limit).await}))
}
async fn get_execution(State(state): State<Arc<GatewayState>>, Path(id): Path<String>) -> Response {
    match state.executions.get(&id).await {
        Some(record) => Json(record).into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "EXECUTION_NOT_FOUND",
            "Execution was not found.",
        ),
    }
}
async fn cancel_execution(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
) -> Response {
    if state.executions.cancel(&id).await {
        Json(json!({"ok":true,"id":id})).into_response()
    } else if state.executions.get(&id).await.is_some() {
        Json(json!({"ok":true,"id":id,"alreadyFinished":true})).into_response()
    } else {
        api_error(
            StatusCode::NOT_FOUND,
            "EXECUTION_NOT_FOUND",
            "Execution was not found.",
        )
    }
}

async fn events(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let cursor = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let mut receiver = state.events.subscribe();
    let history = state
        .event_history
        .lock()
        .map(|events| events.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let current_id = state
        .event_id
        .load(std::sync::atomic::Ordering::Relaxed)
        .saturating_sub(1);
    let missed = cursor.is_some_and(|id| {
        history
            .first()
            .is_some_and(|event| id.saturating_add(1) < event.id)
            || id > current_id
    });
    let initial = if missed {
        vec![GatewayEvent {
            id: current_id,
            event: "resync_required".into(),
            data: json!({"reason":"event cursor is outside the replay window"}),
        }]
    } else {
        history
            .into_iter()
            .filter(|event| cursor.is_none_or(|id| event.id > id))
            .collect()
    };
    let stream = async_stream::stream! {
        let mut last = cursor.unwrap_or(0);
        for event in initial { last = last.max(event.id); yield Ok(to_sse(event)); }
        loop {
            match receiver.recv().await {
                Ok(event) if event.id > last => { last = event.id; yield Ok(to_sse(event)); }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let id = state.event_id.load(std::sync::atomic::Ordering::Relaxed).saturating_sub(1);
                    last = id; yield Ok(Event::default().id(id.to_string()).event("resync_required").data("{\"reason\":\"subscriber fell behind\"}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}
fn to_sse(event: GatewayEvent) -> Event {
    let json = serde_json::to_string(&event.data).unwrap_or_else(|_| "{}".into());
    Event::default()
        .id(event.id.to_string())
        .event(event.event)
        .data(json)
}

async fn create_session(State(state): State<Arc<GatewayState>>, headers: HeaderMap) -> Response {
    let supplied = bearer(&headers);
    let snapshot = state.snapshot().await;
    let admin_expected = token_from_config(snapshot.config.server.auth.admin_token_env.as_deref());
    if admin_expected.as_ref().is_some_and(|expected| {
        !constant_time_equal(supplied.unwrap_or_default().as_bytes(), expected.as_bytes())
    }) || (admin_expected.is_none()
        && (snapshot.config.server.auth.admin_token_env.is_some()
            || !is_loopback_request(&headers)))
    {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Valid admin authentication is required.",
        );
    }
    let session = uuid::Uuid::new_v4().to_string();
    let csrf = uuid::Uuid::new_v4().to_string();
    let expires_at = now_seconds() + 12 * 60 * 60;
    let mut sessions = state.sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > now_seconds());
    sessions.insert(
        session.clone(),
        crate::state::AdminSession {
            csrf: csrf.clone(),
            expires_at,
        },
    );
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https");
    let secure_attribute = if secure { "; Secure" } else { "" };
    let mut response = Json(json!({"ok":true,"expiresAt":expires_at})).into_response();
    let cookie = format!(
        "gateway_session={session}; Path=/api/v1; HttpOnly; SameSite=Strict; Max-Age=43200{secure_attribute}"
    );
    let csrf_cookie = format!(
        "gateway_csrf={csrf}; Path=/api/v1; SameSite=Strict; Max-Age=43200{secure_attribute}"
    );
    response
        .headers_mut()
        .append(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie).unwrap(),
    );
    response
}
async fn delete_session(State(state): State<Arc<GatewayState>>, headers: HeaderMap) -> Response {
    if let Some(session_id) = cookie_value(&headers, "gateway_session") {
        let mut sessions = state.sessions.lock().await;
        if let Some(session) = sessions.get(session_id)
            && (headers.get("x-csrf-token").and_then(|v| v.to_str().ok())
                != Some(session.csrf.as_str())
                || cookie_value(&headers, "gateway_csrf") != Some(session.csrf.as_str()))
        {
            return api_error(
                StatusCode::FORBIDDEN,
                "CSRF_REJECTED",
                "CSRF token is missing or invalid.",
            );
        }
        sessions.remove(session_id);
    }
    let mut response = Json(json!({"ok":true})).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "gateway_session=; Path=/api/v1; HttpOnly; SameSite=Strict; Max-Age=0",
        ),
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static("gateway_csrf=; Path=/api/v1; SameSite=Strict; Max-Age=0"),
    );
    response
}

async fn admin_auth(
    State(state): State<Arc<GatewayState>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let token = bearer(headers);
    let snapshot = state.snapshot().await;
    let expected = token_from_config(snapshot.config.server.auth.admin_token_env.as_deref());
    if expected.as_ref().is_some_and(|expected| {
        constant_time_equal(token.unwrap_or_default().as_bytes(), expected.as_bytes())
    }) {
        return next.run(request).await;
    }
    if let Some(session_id) = cookie_value(headers, "gateway_session") {
        let session = state.sessions.lock().await.get(session_id).cloned();
        if let Some(session) = session.filter(|session| session.expires_at > now_seconds()) {
            let safe = matches!(
                *request.method(),
                Method::GET | Method::HEAD | Method::OPTIONS
            );
            if safe
                || (headers.get("x-csrf-token").and_then(|v| v.to_str().ok())
                    == Some(session.csrf.as_str())
                    && cookie_value(headers, "gateway_csrf") == Some(session.csrf.as_str()))
            {
                return next.run(request).await;
            }
            return api_error(
                StatusCode::FORBIDDEN,
                "CSRF_REJECTED",
                "CSRF token is missing or invalid.",
            );
        }
    }
    if token.is_none()
        && expected.is_none()
        && snapshot.config.server.auth.admin_token_env.is_none()
        && is_loopback_request(headers)
    {
        return next.run(request).await;
    }
    api_error(
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
        "Admin authentication is required.",
    )
}

async fn host_origin_and_client_auth(
    State(state): State<Arc<GatewayState>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    if !host_allowed(&state, headers).await {
        return api_error(
            StatusCode::BAD_REQUEST,
            "HOST_REJECTED",
            "Request Host is not allowed.",
        );
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !origin_allowed(origin, headers)
    {
        return api_error(
            StatusCode::FORBIDDEN,
            "ORIGIN_REJECTED",
            "Request Origin is not allowed.",
        );
    }
    let snapshot = state.snapshot().await;
    if request.uri().path() == snapshot.config.server.mcp_path {
        if let Some(expected) =
            token_from_config(snapshot.config.server.auth.client_token_env.as_deref())
        {
            if !constant_time_equal(
                bearer(headers).unwrap_or_default().as_bytes(),
                expected.as_bytes(),
            ) {
                return api_error(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "MCP client authentication is required.",
                );
            }
        } else if snapshot.config.server.auth.client_token_env.is_some()
            || !is_loopback_request(headers)
        {
            return api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "AUTH_NOT_CONFIGURED",
                "MCP authentication is not configured for remote access.",
            );
        }
    }
    next.run(request).await
}

async fn host_allowed(state: &GatewayState, headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(requested) = host.parse::<http::uri::Authority>() else {
        return false;
    };
    let snapshot = state.snapshot().await;
    if let Ok(listen) = snapshot.config.server.listen.parse::<SocketAddr>()
        && listen.ip().is_loopback()
        && is_loopback_host(requested.host())
        && requested
            .port_u16()
            .is_none_or(|port| port == listen.port())
    {
        return true;
    }
    let allowed = std::env::var("GATEWAY_ALLOWED_HOSTS").unwrap_or_default();
    allowed
        .split(',')
        .map(str::trim)
        .any(|item| host_matches(item, requested.as_ref()))
}
fn origin_allowed(origin: &str, headers: &HeaderMap) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") || url.username() != "" || url.password().is_some()
    {
        return false;
    }
    if std::env::var("GATEWAY_ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .any(|value| !value.is_empty() && value == origin)
    {
        return true;
    }
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(host_authority) = host.parse::<http::uri::Authority>().ok() else {
        return false;
    };
    let Some(origin_host) = url.host_str() else {
        return false;
    };
    let origin_port = url.port_or_known_default();
    let host_port = host_authority
        .port_u16()
        .or_else(|| Some(if url.scheme() == "https" { 443 } else { 80 }));
    origin_port == host_port
        && (origin_host.eq_ignore_ascii_case(host_authority.host())
            || (is_loopback_host(origin_host) && is_loopback_host(host_authority.host())))
}
fn host_matches(allowed: &str, requested: &str) -> bool {
    if allowed.is_empty() || allowed == "*" {
        return false;
    }
    let (Ok(allowed), Ok(requested)) = (
        allowed.parse::<http::uri::Authority>(),
        requested.parse::<http::uri::Authority>(),
    ) else {
        return false;
    };
    allowed.host().eq_ignore_ascii_case(requested.host())
        && allowed
            .port_u16()
            .is_none_or(|port| requested.port_u16() == Some(port))
}
fn is_loopback_host(host: &str) -> bool {
    let authority_host = host
        .parse::<http::uri::Authority>()
        .ok()
        .map(|authority| authority.host().to_owned())
        .unwrap_or_else(|| host.to_owned());
    let name = authority_host.trim_matches(['[', ']']);
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}
fn is_loopback_request(headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_loopback_host)
}
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}
fn token_from_config(env_name: Option<&str>) -> Option<String> {
    env_name
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
}
fn cookie_value<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| if name == key { Some(value) } else { None })
}
fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        diff |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    diff == 0
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
fn api_error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"code":code,"message":message}})),
    )
        .into_response()
}
