use crate::{
    registry,
    state::{GatewayState, Snapshot},
};
use anyhow::{Context, Result, anyhow};
use deno_core::{Extension, JsRuntime, OpState, PollEventLoopOptions, RuntimeOptions, op2};
use deno_error::JsErrorBox;
use futures::future::BoxFuture;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    io,
    process::Stdio,
    rc::Rc,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, Notify, Semaphore, mpsc, oneshot},
    task::LocalSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_CODE_BYTES: usize = 64 * 1024;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_TOOL_REQUEST_IPC_BYTES: usize = MAX_ARGUMENT_BYTES + 4096;
const MAX_UPSTREAM_RESULT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EXECUTION_RESULT_BYTES: usize = 8 * 1024 * 1024;
const MAX_IPC_LINE_BYTES: usize = 9 * 1024 * 1024;
const EXECUTE_DESCRIPTION: &str = "Execute JavaScript to call and combine upstream MCP tools.\nUse tools_search first to discover tool names and argument schemas.\n\nWrite code as the body of an async function: await and return are supported.\nCall a tool with: await tools.call(\"namespace.name\", arguments).\nEach successful call returns { content, structuredContent?, isError }.\nUse structuredContent when available; otherwise inspect content explicitly.\nDo not assume text content is JSON. Tool failures reject with an error that\ncan be handled using try/catch. An upstream tool error may include error.result.\n\nAwait every tool call. Use Promise.all for independent calls and sequential\nawait for dependent calls. Return only the JSON-serializable data the user\nneeds; intermediate values and console output are not returned automatically.\nNo filesystem, direct network, environment, process, imports, npm or Node APIs\nare available. Execution and tool calls are subject to time and resource limits.\nCancellation does not undo upstream side effects. Do not retry writes blindly.\n\nExample:\nconst result = await tools.call(\"github.list_issues\", {\n  owner: \"example\", repo: \"demo\"\n});\nreturn result.structuredContent ?? result.content;";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub id: String,
    pub status: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub duration_ms: Option<u64>,
    pub code_bytes: usize,
    pub output_bytes: usize,
    pub error_code: Option<String>,
    pub revision: String,
    pub calls: Vec<ExecutionCallRecord>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionCallRecord {
    pub tool_name: String,
    pub decision: String,
    pub matched_pattern: Option<String>,
    pub duration_ms: u64,
    pub bytes: usize,
    pub error_code: Option<String>,
}

pub struct ExecutionManager {
    records: Mutex<HashMap<String, ExecutionRecord>>,
    cancellations: Mutex<HashMap<String, CancellationToken>>,
    active: AtomicUsize,
    queued: AtomicUsize,
    notify: Notify,
    sandbox_factory: Arc<dyn SandboxBackendFactory>,
}

trait SandboxBackend: Send + Sync {
    fn execute(
        &self,
        code: String,
        output_limit: usize,
        heap_limit_mb: usize,
        memory_limit_mb: usize,
        context: Arc<ExecutionContext>,
    ) -> BoxFuture<'static, std::result::Result<Value, ToolError>>;
}

struct DenoBackend;
impl SandboxBackend for DenoBackend {
    fn execute(
        &self,
        code: String,
        output_limit: usize,
        heap_limit_mb: usize,
        memory_limit_mb: usize,
        context: Arc<ExecutionContext>,
    ) -> BoxFuture<'static, std::result::Result<Value, ToolError>> {
        Box::pin(run_worker_process(
            code,
            output_limit,
            heap_limit_mb,
            memory_limit_mb,
            context,
        ))
    }
}

trait SandboxBackendFactory: Send + Sync {
    fn build(&self, provider: &str) -> Result<Box<dyn SandboxBackend>>;
}

struct DenoSandboxFactory;
impl SandboxBackendFactory for DenoSandboxFactory {
    fn build(&self, provider: &str) -> Result<Box<dyn SandboxBackend>> {
        match provider {
            "deno" => Ok(Box::new(DenoBackend)),
            other => anyhow::bail!("sandbox provider is not implemented: {other}"),
        }
    }
}

impl Default for ExecutionManager {
    fn default() -> Self {
        Self::with_factory(Arc::new(DenoSandboxFactory))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkerMessage {
    ToolCall {
        id: u64,
        name: String,
        arguments: Value,
    },
    Finished {
        result: Option<Value>,
        error: Option<ToolError>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkerInput {
    Start {
        code: String,
        output_limit: usize,
        heap_limit: usize,
    },
    ToolResult {
        id: u64,
        ok: bool,
        value: Option<Value>,
        error: Option<ToolError>,
    },
}

type PendingCall = (
    oneshot::Sender<WorkerInput>,
    tokio::sync::OwnedSemaphorePermit,
);
type PendingCalls = Arc<StdMutex<HashMap<u64, PendingCall>>>;

struct ExecutionBridge {
    next_id: AtomicU64,
    outbound: mpsc::Sender<WorkerMessage>,
    pending: PendingCalls,
    pending_slots: Arc<Semaphore>,
}

#[op2]
#[string]
async fn op_gateway_call(
    state: Rc<RefCell<OpState>>,
    #[string] call_json: String,
) -> Result<String, JsErrorBox> {
    if call_json.len() > MAX_TOOL_REQUEST_IPC_BYTES {
        return Err(JsErrorBox::type_error(
            "tool arguments exceed the IPC limit",
        ));
    }
    let (name, arguments): (String, Value) = serde_json::from_str(&call_json)
        .map_err(|_| JsErrorBox::type_error("invalid tools.call request"))?;
    let bridge = state.borrow().borrow::<Arc<ExecutionBridge>>().clone();
    let permit = bridge
        .pending_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| JsErrorBox::type_error("tool broker disconnected"))?;
    let id = bridge.next_id.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = oneshot::channel();
    bridge.pending.lock().unwrap().insert(id, (sender, permit));
    if bridge
        .outbound
        .send(WorkerMessage::ToolCall {
            id,
            name,
            arguments,
        })
        .await
        .is_err()
    {
        bridge.pending.lock().unwrap().remove(&id);
        return Err(JsErrorBox::type_error("tool broker disconnected"));
    }
    let response = receiver
        .await
        .map_err(|_| JsErrorBox::type_error("tool broker disconnected"))?;
    let encoded = match response {
        WorkerInput::ToolResult {
            ok, value, error, ..
        } => json!({"ok":ok,"value":value,"error":error}),
        WorkerInput::Start { .. } => {
            return Err(JsErrorBox::type_error("invalid tool broker response"));
        }
    };
    serde_json::to_string(&encoded)
        .map_err(|_| JsErrorBox::type_error("tool result could not be serialized"))
}

deno_core::extension!(gateway_extension, ops = [op_gateway_call]);

impl ExecutionManager {
    fn with_factory(sandbox_factory: Arc<dyn SandboxBackendFactory>) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            cancellations: Mutex::new(HashMap::new()),
            active: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            notify: Notify::new(),
            sandbox_factory,
        }
    }

    pub async fn restore(&self, db: &Connection) -> Result<()> {
        let mut restored = HashMap::new();
        {
            let mut statement = db.prepare("SELECT id,outcome,CAST(strftime('%s',started_at) AS INTEGER)*1000,CASE WHEN finished_at IS NULL THEN NULL ELSE CAST(strftime('%s',finished_at) AS INTEGER)*1000 END,duration_ms,code_bytes,output_bytes,error_code,revision FROM executions ORDER BY started_at DESC LIMIT 2000")?;
            let rows = statement.query_map([], |row| {
                let started_at: i64 = row.get(2)?;
                let finished_at: Option<i64> = row.get(3)?;
                let duration: Option<i64> = row.get(4)?;
                let code_bytes: i64 = row.get(5)?;
                let output_bytes: i64 = row.get(6)?;
                Ok(ExecutionRecord {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    started_at: started_at.max(0) as u64,
                    finished_at: finished_at.map(|value| value.max(0) as u64),
                    duration_ms: duration.map(|value| value.max(0) as u64),
                    code_bytes: code_bytes.max(0) as usize,
                    output_bytes: output_bytes.max(0) as usize,
                    error_code: row.get(7)?,
                    revision: row.get(8)?,
                    calls: Vec::new(),
                })
            })?;
            for row in rows {
                let record = row?;
                restored.insert(record.id.clone(), record);
            }
        }
        {
            let mut statement = db.prepare("SELECT execution_id,tool_name,decision,matched_pattern,duration_ms,bytes,error_code FROM execution_events WHERE kind='tool_call' ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ExecutionCallRecord {
                        tool_name: row.get(1)?,
                        decision: row.get(2)?,
                        matched_pattern: row.get(3)?,
                        duration_ms: row.get::<_, Option<i64>>(4)?.unwrap_or_default().max(0)
                            as u64,
                        bytes: row.get::<_, Option<i64>>(5)?.unwrap_or_default().max(0) as usize,
                        error_code: row.get(6)?,
                    },
                ))
            })?;
            for row in rows {
                let (execution_id, call) = row?;
                if let Some(record) = restored.get_mut(&execution_id) {
                    record.calls.push(call);
                }
            }
        }
        *self.records.lock().await = restored;
        Ok(())
    }

    pub async fn run(
        self: &Arc<Self>,
        gateway: Arc<GatewayState>,
        code: String,
        timeout_ms: Option<u64>,
        external_cancel: Option<CancellationToken>,
    ) -> Value {
        let initial = gateway.snapshot().await;
        let settings = initial.config.sandbox.clone();
        let max_time = timeout_ms.unwrap_or(settings.timeout_ms);
        if code.len() > MAX_CODE_BYTES {
            return error_envelope("CODE_TOO_LARGE", "JavaScript code exceeds 64 KiB", None);
        }
        if max_time == 0 || max_time > settings.timeout_ms {
            return error_envelope(
                "INVALID_TIMEOUT",
                "timeout_ms must be positive and within the configured limit",
                None,
            );
        }
        if settings.max_output_bytes > MAX_EXECUTION_RESULT_BYTES {
            return error_envelope(
                "INVALID_CONFIGURATION",
                "configured output limit exceeds 8 MiB",
                None,
            );
        }

        let started = Instant::now();
        let deadline = started + Duration::from_millis(max_time);
        let id = Uuid::new_v4().to_string();
        let cancel = external_cancel
            .map(|token| token.child_token())
            .unwrap_or_default();
        let record = ExecutionRecord {
            id: id.clone(),
            status: "queued".into(),
            started_at: epoch_millis(),
            finished_at: None,
            duration_ms: None,
            code_bytes: code.len(),
            output_bytes: 0,
            error_code: None,
            revision: initial.revision.clone(),
            calls: Vec::new(),
        };
        self.records.lock().await.insert(id.clone(), record);
        self.cancellations
            .lock()
            .await
            .insert(id.clone(), cancel.clone());
        persist_start(&gateway, &id, code.len(), &initial.revision).await;
        gateway.publish("execution.queued", json!({"id": id}));

        if let Err(reason) = self
            .acquire_slot(
                settings.max_concurrent_executions,
                settings.max_queue_size,
                &cancel,
                deadline,
            )
            .await
        {
            let (code, status, message) = match reason {
                AcquireError::QueueFull => ("QUEUE_FULL", "failed", "Execution queue is full."),
                AcquireError::Cancelled => ("CANCELLED", "cancelled", "Execution was cancelled."),
                AcquireError::Timeout => ("EXECUTION_TIMEOUT", "timed_out", "Execution timed out."),
            };
            self.finish(&gateway, &id, status, Some(code), 0).await;
            return error_envelope(code, message, None);
        }

        let active_guard = ActiveGuard {
            manager: self.clone(),
        };
        if cancel.is_cancelled() {
            drop(active_guard);
            self.finish(&gateway, &id, "cancelled", Some("CANCELLED"), 0)
                .await;
            return error_envelope("CANCELLED", "Execution was cancelled.", None);
        }
        self.update_status(&gateway, &id, "running", None).await;
        let execution = Arc::new(ExecutionContext {
            gateway: gateway.clone(),
            id: id.clone(),
            base: initial,
            cancel: cancel.clone(),
            calls: AtomicUsize::new(0),
            parallel: Arc::new(Semaphore::new(settings.max_parallel_tool_calls)),
            records: Mutex::new(Vec::new()),
            total_result_bytes: AtomicUsize::new(0),
            max_calls: settings.max_tool_calls,
            deadline,
            pending_jobs: AtomicUsize::new(0),
            pending_notify: Notify::new(),
        });
        let backend = match self.sandbox_factory.build(&settings.provider) {
            Ok(backend) => backend,
            Err(_) => {
                drop(active_guard);
                self.finish(&gateway, &id, "failed", Some("SANDBOX_UNAVAILABLE"), 0)
                    .await;
                return error_envelope(
                    "SANDBOX_UNAVAILABLE",
                    "Configured JavaScript sandbox is unavailable.",
                    None,
                );
            }
        };
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(ToolError { code: "CANCELLED".into(), message: "Execution was cancelled.".into(), result: None }),
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => { cancel.cancel(); Err(ToolError { code: "EXECUTION_TIMEOUT".into(), message: "Execution timed out.".into(), result: None }) },
            result = backend.execute(code.clone(), settings.max_output_bytes, settings.options.heap_limit_mb, settings.options.worker_memory_limit_mb, execution.clone()) => result,
        };
        if result.is_err() {
            cancel.cancel();
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), execution.wait_for_pending()).await;
        drop(active_guard);
        let calls = execution.records.lock().await.clone();
        self.set_calls(&id, calls.clone()).await;
        for call in &calls {
            persist_call(&gateway, &id, call).await;
        }
        match result {
            Ok(value) => {
                let bytes = serde_json::to_vec(&value)
                    .map(|v| v.len())
                    .unwrap_or(usize::MAX);
                if bytes > settings.max_output_bytes {
                    self.finish(&gateway, &id, "failed", Some("OUTPUT_TOO_LARGE"), bytes)
                        .await;
                    error_envelope(
                        "OUTPUT_TOO_LARGE",
                        "Execution result exceeds the configured output limit.",
                        None,
                    )
                } else {
                    self.finish(&gateway, &id, "succeeded", None, bytes).await;
                    json!({"result": value})
                }
            }
            Err(error) => {
                let status = if error.code == "EXECUTION_TIMEOUT" {
                    "timed_out"
                } else if error.code == "CANCELLED" {
                    "cancelled"
                } else {
                    "failed"
                };
                self.finish(&gateway, &id, status, Some(&error.code), 0)
                    .await;
                error_envelope(&error.code, &error.message, error.result)
            }
        }
    }

    async fn acquire_slot(
        &self,
        limit: usize,
        max_queue: usize,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> std::result::Result<(), AcquireError> {
        let mut waiting = false;
        loop {
            let notified = self.notify.notified();
            let active = self.active.load(Ordering::Acquire);
            if active < limit
                && self
                    .active
                    .compare_exchange(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                if waiting {
                    self.queued.fetch_sub(1, Ordering::AcqRel);
                }
                return Ok(());
            }
            if !waiting {
                let queued = self.queued.fetch_add(1, Ordering::AcqRel);
                if queued >= max_queue {
                    self.queued.fetch_sub(1, Ordering::AcqRel);
                    return Err(AcquireError::QueueFull);
                }
                waiting = true;
            }
            let reason = tokio::select! { _ = cancel.cancelled() => Some(AcquireError::Cancelled), _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Some(AcquireError::Timeout), _ = notified => None };
            if let Some(reason) = reason {
                self.queued.fetch_sub(1, Ordering::AcqRel);
                return Err(reason);
            }
        }
    }

    pub async fn cancel(&self, id: &str) -> bool {
        if let Some(token) = self.cancellations.lock().await.get(id) {
            token.cancel();
            true
        } else {
            false
        }
    }
    pub async fn list(&self, limit: usize) -> Vec<ExecutionRecord> {
        let records = self.records.lock().await;
        let mut values: Vec<_> = records.values().cloned().collect();
        values.sort_by_key(|record| std::cmp::Reverse(record.started_at));
        values.truncate(limit.min(500));
        values
    }
    pub async fn get(&self, id: &str) -> Option<ExecutionRecord> {
        self.records.lock().await.get(id).cloned()
    }
    async fn set_calls(&self, id: &str, calls: Vec<ExecutionCallRecord>) {
        if let Some(record) = self.records.lock().await.get_mut(id) {
            record.calls = calls;
        }
    }
    async fn update_status(
        &self,
        gateway: &GatewayState,
        id: &str,
        status: &str,
        code: Option<&str>,
    ) {
        if let Some(record) = self.records.lock().await.get_mut(id) {
            record.status = status.into();
            record.error_code = code.map(str::to_owned);
        }
        gateway.publish("execution.started", json!({"id": id}));
        let db = gateway.db.lock().await;
        let _ = db.execute(
            "UPDATE executions SET outcome=?2,error_code=?3 WHERE id=?1",
            rusqlite::params![id, status, code],
        );
    }
    async fn finish(
        &self,
        gateway: &GatewayState,
        id: &str,
        status: &str,
        code: Option<&str>,
        bytes: usize,
    ) {
        let now = epoch_millis();
        let mut duration = 0;
        if let Some(record) = self.records.lock().await.get_mut(id) {
            duration = now.saturating_sub(record.started_at);
            record.status = status.into();
            record.finished_at = Some(now);
            record.duration_ms = Some(duration);
            record.error_code = code.map(str::to_owned);
            record.output_bytes = bytes;
        }
        self.cancellations.lock().await.remove(id);
        gateway.publish(
            "execution.finished",
            json!({"id": id, "status": status, "durationMs": duration, "errorCode": code}),
        );
        let db = gateway.db.lock().await;
        let _ = db.execute("UPDATE executions SET finished_at=CURRENT_TIMESTAMP,outcome=?2,error_code=?3,output_bytes=?4,duration_ms=?5 WHERE id=?1", rusqlite::params![id, status, code, bytes as i64, duration as i64]);
    }
}

enum AcquireError {
    QueueFull,
    Cancelled,
    Timeout,
}

struct ActiveGuard {
    manager: Arc<ExecutionManager>,
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.manager.active.fetch_sub(1, Ordering::AcqRel);
        self.manager.notify.notify_one();
    }
}

struct ExecutionContext {
    gateway: Arc<GatewayState>,
    id: String,
    base: Arc<Snapshot>,
    cancel: CancellationToken,
    calls: AtomicUsize,
    parallel: Arc<Semaphore>,
    records: Mutex<Vec<ExecutionCallRecord>>,
    total_result_bytes: AtomicUsize,
    max_calls: usize,
    deadline: Instant,
    pending_jobs: AtomicUsize,
    pending_notify: Notify,
}

impl ExecutionContext {
    async fn dispatch(
        self: &Arc<Self>,
        name: String,
        args: Value,
    ) -> std::result::Result<Value, ToolError> {
        let started = Instant::now();
        let call_number = self.calls.fetch_add(1, Ordering::AcqRel) + 1;
        if call_number > self.max_calls {
            return self
                .record_error(
                    name,
                    started,
                    "TOOL_CALL_LIMIT",
                    "Execution tool call limit reached.",
                    None,
                    None,
                )
                .await;
        }
        if self.cancel.is_cancelled() {
            return self
                .record_error(
                    name,
                    started,
                    "CANCELLED",
                    "Execution was cancelled.",
                    None,
                    None,
                )
                .await;
        }
        let tool_name = name.clone();
        let Some((namespace, _)) = name.split_once('.') else {
            return self
                .record_error(
                    name,
                    started,
                    "TOOL_NOT_FOUND",
                    "Tool name must be namespace.name.",
                    None,
                    None,
                )
                .await;
        };
        let current = self.gateway.snapshot().await;
        let decision = crate::policy::evaluate(&current.config.policy, &tool_name);
        if !decision.allowed {
            return self
                .record_error(
                    name,
                    started,
                    "POLICY_DENIED",
                    "Tool is denied by gateway policy.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let Some(tool) = current.registry.tools.get(&tool_name) else {
            let changed = self.base.registry.tools.contains_key(&tool_name);
            let code = if changed {
                "SCHEMA_CHANGED"
            } else {
                "TOOL_UNAVAILABLE"
            };
            let message = if changed {
                "Tool mapping changed; search for the current schema and try again."
            } else {
                "Tool is unavailable or not found."
            };
            return self
                .record_error(name, started, code, message, decision.matched_pattern, None)
                .await;
        };
        if tool.generation != self.base.generation {
            let upstream_unchanged = self.base.config.upstreams.get(namespace)
                == current.config.upstreams.get(namespace);
            let unchanged = self.base.registry.tools.get(&tool_name).is_some_and(|old| {
                old.input_schema == tool.input_schema
                    && old.output_schema == tool.output_schema
                    && old.description == tool.description
            }) && upstream_unchanged;
            if !unchanged {
                return self
                    .record_error(
                        name,
                        started,
                        "SCHEMA_CHANGED",
                        "Tool schema changed; search for the current schema and try again.",
                        decision.matched_pattern,
                        None,
                    )
                    .await;
            }
        }
        let Some(status) = current.registry.upstreams.get(namespace) else {
            return self
                .record_error(
                    name,
                    started,
                    "UPSTREAM_UNAVAILABLE",
                    "Upstream is unavailable.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        };
        if !status.available || !status.enabled {
            return self
                .record_error(
                    name,
                    started,
                    "UPSTREAM_UNAVAILABLE",
                    "Upstream is unavailable.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let args_bytes = serde_json::to_vec(&args)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if args_bytes > MAX_ARGUMENT_BYTES {
            return self
                .record_error(
                    name,
                    started,
                    "ARGUMENTS_TOO_LARGE",
                    "Tool arguments exceed 256 KiB.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        if !args.is_object() || !tool.input_validator.is_valid(&args) {
            return self
                .record_error(
                    name,
                    started,
                    "INVALID_ARGUMENTS",
                    "Tool arguments do not match the current input schema.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let upstream_semaphore = current
            .registry
            .upstream_limits
            .get(namespace)
            .cloned()
            .unwrap_or_else(|| Arc::new(Semaphore::new(1)));
        let upstream_permit = tokio::select! { _ = self.cancel.cancelled() => return self.record_error(name, started, "CANCELLED", "Execution was cancelled.", decision.matched_pattern, None).await,
        result = upstream_semaphore.acquire_owned() => match result { Ok(permit) => permit, Err(_) => return self.record_error(name, started, "UPSTREAM_UNAVAILABLE", "Upstream is unavailable.", decision.matched_pattern, None).await } };
        let permit = tokio::select! { _ = self.cancel.cancelled() => return self.record_error(name, started, "CANCELLED", "Execution was cancelled.", decision.matched_pattern, None).await,
        result = self.parallel.clone().acquire_owned() => match result { Ok(permit) => permit, Err(_) => return self.record_error(name, started, "TOOL_CALL_LIMIT", "Tool call limit reached.", decision.matched_pattern, None).await } };
        // A call may wait for a permit while a config reload changes policy or schema.
        // Check the published snapshot again at the broker admission point.
        let latest = self.gateway.snapshot().await;
        let decision = crate::policy::evaluate(&latest.config.policy, &tool_name);
        if !decision.allowed {
            return self
                .record_error(
                    name,
                    started,
                    "POLICY_DENIED",
                    "Tool is denied by gateway policy.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let Some(latest_tool) = latest.registry.tools.get(&tool_name) else {
            return self
                .record_error(
                    name,
                    started,
                    "SCHEMA_CHANGED",
                    "Tool mapping changed; search for the current schema and try again.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        };
        if tool.input_schema != latest_tool.input_schema
            || tool.output_schema != latest_tool.output_schema
            || current.config.upstreams.get(namespace) != latest.config.upstreams.get(namespace)
        {
            return self
                .record_error(
                    name,
                    started,
                    "SCHEMA_CHANGED",
                    "Tool schema changed; search for the current schema and try again.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        if !latest
            .registry
            .upstreams
            .get(namespace)
            .is_some_and(|status| status.enabled && status.available)
        {
            return self
                .record_error(
                    name,
                    started,
                    "UPSTREAM_UNAVAILABLE",
                    "Upstream is unavailable.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let Some(client) = latest.registry.clients.get(namespace) else {
            return self
                .record_error(
                    name,
                    started,
                    "UPSTREAM_UNAVAILABLE",
                    "Upstream is unavailable.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        };
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            drop(permit);
            return self
                .record_error(
                    name,
                    started,
                    "EXECUTION_TIMEOUT",
                    "Execution timed out.",
                    decision.matched_pattern,
                    None,
                )
                .await;
        }
        let call_deadline = remaining.min(Duration::from_secs(15));
        let result = tokio::select! {
            _ = self.cancel.cancelled() => return self.record_unknown(name, started, "CANCELLED", "Execution was cancelled.", decision.matched_pattern.clone()).await,
            result = tokio::time::timeout(call_deadline, registry::call(client, latest_tool, args)) => match result { Ok(result) => result.map_err(|e| anyhow!("{}", e)), Err(_) => Err(anyhow!("UPSTREAM_TIMEOUT")) }
        };
        drop(permit);
        drop(upstream_permit);
        match result {
            Ok(result) => {
                let is_error = result.is_error.unwrap_or(false);
                let mut envelope = json!({"content": result.content, "isError": is_error});
                if let (Some(object), Some(structured)) =
                    (envelope.as_object_mut(), result.structured_content)
                {
                    object.insert("structuredContent".into(), structured);
                }
                let bytes = serde_json::to_vec(&envelope)
                    .map(|b| b.len())
                    .unwrap_or(usize::MAX);
                if bytes > MAX_UPSTREAM_RESULT_BYTES {
                    return self
                        .record_error(
                            name,
                            started,
                            "UPSTREAM_RESULT_TOO_LARGE",
                            "Upstream tool result exceeds 2 MiB.",
                            decision.matched_pattern,
                            None,
                        )
                        .await;
                }
                let cumulative = self
                    .total_result_bytes
                    .fetch_add(bytes, Ordering::AcqRel)
                    .saturating_add(bytes);
                if cumulative > MAX_EXECUTION_RESULT_BYTES {
                    return self
                        .record_error(
                            name,
                            started,
                            "EXECUTION_RESULT_LIMIT",
                            "Execution tool result budget was exceeded.",
                            decision.matched_pattern,
                            None,
                        )
                        .await;
                }
                if is_error {
                    self.record_error(
                        name,
                        started,
                        "UPSTREAM_TOOL_ERROR",
                        "Upstream tool returned an error.",
                        decision.matched_pattern,
                        Some(envelope),
                    )
                    .await
                } else {
                    self.record_success(name, started, decision.matched_pattern, bytes)
                        .await;
                    Ok(envelope)
                }
            }
            Err(error) => {
                let raw = error.to_string();
                let (code, message) = if raw.contains("INVALID_ARGUMENTS:") {
                    (
                        "INVALID_ARGUMENTS",
                        "Tool arguments do not match the current input schema.",
                    )
                } else if raw.contains("UPSTREAM_RESULT_TOO_LARGE") {
                    (
                        "UPSTREAM_RESULT_TOO_LARGE",
                        "Upstream tool result exceeds 2 MiB.",
                    )
                } else if raw.contains("UPSTREAM_TIMEOUT") {
                    ("UPSTREAM_TIMEOUT", "Upstream call timed out.")
                } else if raw.contains("CANCELLED") {
                    ("CANCELLED", "Execution was cancelled.")
                } else {
                    ("UPSTREAM_TRANSPORT_ERROR", "Upstream tool call failed.")
                };
                if matches!(code, "UPSTREAM_TIMEOUT" | "UPSTREAM_TRANSPORT_ERROR") {
                    self.record_unknown(name, started, code, message, decision.matched_pattern)
                        .await
                } else {
                    self.record_error(name, started, code, message, decision.matched_pattern, None)
                        .await
                }
            }
        }
    }

    async fn record_success(
        &self,
        name: String,
        started: Instant,
        matched: Option<String>,
        bytes: usize,
    ) {
        let event = ExecutionCallRecord {
            tool_name: name,
            decision: "allow".into(),
            matched_pattern: matched,
            duration_ms: started.elapsed().as_millis() as u64,
            bytes,
            error_code: None,
        };
        self.records.lock().await.push(event.clone());
        self.gateway.publish("execution.call", json!({"id": self.id, "toolName": event.tool_name, "decision": event.decision, "bytes": bytes}));
    }
    async fn record_error(
        &self,
        name: String,
        started: Instant,
        code: &str,
        message: &str,
        matched: Option<String>,
        result: Option<Value>,
    ) -> std::result::Result<Value, ToolError> {
        let decision = if code == "POLICY_DENIED" {
            "deny"
        } else {
            "error"
        };
        self.record_error_decision(name, started, code, message, matched, result, decision)
            .await
    }
    async fn record_unknown(
        &self,
        name: String,
        started: Instant,
        code: &str,
        message: &str,
        matched: Option<String>,
    ) -> std::result::Result<Value, ToolError> {
        self.record_error_decision(
            name,
            started,
            code,
            message,
            matched,
            None,
            "outcome_unknown",
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn record_error_decision(
        &self,
        name: String,
        started: Instant,
        code: &str,
        message: &str,
        matched: Option<String>,
        result: Option<Value>,
        decision: &str,
    ) -> std::result::Result<Value, ToolError> {
        let event = ExecutionCallRecord {
            tool_name: name,
            decision: decision.into(),
            matched_pattern: matched,
            duration_ms: started.elapsed().as_millis() as u64,
            bytes: result
                .as_ref()
                .and_then(|v| serde_json::to_vec(v).ok())
                .map(|b| b.len())
                .unwrap_or(0),
            error_code: Some(code.into()),
        };
        self.records.lock().await.push(event.clone());
        self.gateway.publish("execution.call", json!({"id": self.id, "toolName": event.tool_name, "decision": event.decision, "bytes": event.bytes, "errorCode": code}));
        Err(ToolError {
            code: code.into(),
            message: message.into(),
            result,
        })
    }
    async fn wait_for_pending(&self) {
        loop {
            let notified = self.pending_notify.notified();
            if self.pending_jobs.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

struct PendingJobGuard(Arc<ExecutionContext>);
impl Drop for PendingJobGuard {
    fn drop(&mut self) {
        if self.0.pending_jobs.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.pending_notify.notify_one();
        }
    }
}

async fn run_worker_process(
    code: String,
    output_limit: usize,
    heap_limit_mb: usize,
    memory_limit_mb: usize,
    context: Arc<ExecutionContext>,
) -> std::result::Result<Value, ToolError> {
    if !sysinfo::IS_SUPPORTED_SYSTEM {
        return Err(worker_failure("WORKER_MEMORY_LIMIT_UNAVAILABLE"));
    }
    let binary = std::env::current_exe().map_err(|_| ToolError {
        code: "SANDBOX_START_FAILED".into(),
        message: "Could not start JavaScript worker.".into(),
        result: None,
    })?;
    let mut child = Command::new(binary)
        .arg("--sandbox-worker")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| ToolError {
            code: "SANDBOX_START_FAILED".into(),
            message: "Could not start JavaScript worker.".into(),
            result: None,
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| worker_failure("SANDBOX_START_FAILED"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| worker_failure("SANDBOX_START_FAILED"))?;
    write_frame(
        &mut stdin,
        &WorkerInput::Start {
            code,
            output_limit,
            heap_limit: heap_limit_mb.saturating_mul(1024 * 1024),
        },
    )
    .await
    .map_err(|_| worker_failure("SANDBOX_START_FAILED"))?;
    let writer = Arc::new(Mutex::new(stdin));
    let (frame_sender, mut frames) = mpsc::channel::<Vec<u8>>(1);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut pending_frame = Vec::new();
        while let Ok(Some(frame)) =
            read_frame(&mut reader, &mut pending_frame, MAX_IPC_LINE_BYTES).await
        {
            if frame_sender.send(frame).await.is_err() {
                break;
            }
        }
    });
    let Some(pid) = child.id().map(Pid::from_u32) else {
        terminate(&mut child).await;
        return Err(worker_failure("WORKER_MEMORY_LIMIT_UNAVAILABLE"));
    };
    let pid_slice = [pid];
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&pid_slice), false);
    if system.process(pid).is_none() {
        terminate(&mut child).await;
        return Err(worker_failure("WORKER_MEMORY_LIMIT_UNAVAILABLE"));
    }
    let memory_limit = memory_limit_mb.saturating_mul(1024 * 1024) as u64;
    let mut memory_watch = tokio::time::interval(Duration::from_millis(10));
    memory_watch.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let line = tokio::select! {
            biased;
            _ = context.cancel.cancelled() => { terminate(&mut child).await; return Err(worker_failure("CANCELLED")); },
            line = frames.recv() => {
                let Some(line) = line else {
                    let _ = child.wait().await;
                    return Err(worker_failure("SANDBOX_WORKER_CRASHED"));
                };
                Some(line)
            },
            _ = memory_watch.tick() => {
                system.refresh_processes(ProcessesToUpdate::Some(&pid_slice), false);
                match system.process(pid) {
                    Some(process) if process.memory() <= memory_limit => continue,
                    Some(_) => { terminate(&mut child).await; return Err(worker_failure("WORKER_MEMORY_LIMIT")); },
                    None => { terminate(&mut child).await; return Err(worker_failure("WORKER_MEMORY_LIMIT_UNAVAILABLE")); }
                }
            }
        };
        let Some(line) = line else {
            let _ = child.wait().await;
            return Err(worker_failure("SANDBOX_WORKER_CRASHED"));
        };
        let message: WorkerMessage =
            serde_json::from_slice(&line).map_err(|_| worker_failure("SANDBOX_WORKER_CRASHED"))?;
        match message {
            WorkerMessage::ToolCall {
                id,
                name,
                arguments,
            } => {
                let context = context.clone();
                context.pending_jobs.fetch_add(1, Ordering::AcqRel);
                let writer_job = writer.clone();
                tokio::spawn(async move {
                    let _pending = PendingJobGuard(context.clone());
                    let (ok, value, error) = match context.dispatch(name, arguments).await {
                        Ok(value) => (true, Some(value), None),
                        Err(error) => (false, None, Some(error)),
                    };
                    let mut writer = writer_job.lock().await;
                    let _ = write_frame(
                        &mut writer,
                        &WorkerInput::ToolResult {
                            id,
                            ok,
                            value,
                            error,
                        },
                    )
                    .await;
                });
            }
            WorkerMessage::Finished { result, error } => {
                drop(writer);
                let _ = child.wait().await;
                if let Some(mut error) = error {
                    error.result = None;
                    return Err(sanitize_script_error(error));
                }
                return Ok(result.unwrap_or(Value::Null));
            }
        }
    }
}

fn worker_failure(code: &str) -> ToolError {
    let message = match code {
        "CANCELLED" => "Execution was cancelled.",
        "EXECUTION_TIMEOUT" => "Execution timed out.",
        "RESULT_NOT_SERIALIZABLE" => "Script result is not JSON serializable.",
        "WORKER_MEMORY_LIMIT" => "JavaScript worker exceeded its memory limit.",
        "WORKER_MEMORY_LIMIT_UNAVAILABLE" => {
            "JavaScript worker memory limit could not be enforced."
        }
        _ => "JavaScript worker stopped unexpectedly.",
    };
    ToolError {
        code: code.into(),
        message: message.into(),
        result: None,
    }
}

fn sanitize_script_error(mut error: ToolError) -> ToolError {
    let code = match error.code.as_str() {
        "UPSTREAM_TOOL_ERROR" | "RESULT_NOT_SERIALIZABLE" | "OUTPUT_TOO_LARGE" => {
            error.code.as_str()
        }
        _ => "JAVASCRIPT_ERROR",
    }
    .to_owned();
    let message = match code.as_str() {
        "UPSTREAM_TOOL_ERROR" => "Upstream tool returned an error.",
        "RESULT_NOT_SERIALIZABLE" => "Script result is not JSON serializable.",
        "OUTPUT_TOO_LARGE" => "Execution result exceeds the configured output limit.",
        _ => "JavaScript execution failed.",
    };
    error.code = code;
    error.message = message.into();
    error.result = None;
    error
}

async fn terminate(child: &mut Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
}

async fn write_frame(writer: &mut ChildStdin, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_IPC_LINE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "worker frame too large",
        ));
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

async fn read_frame(
    reader: &mut BufReader<ChildStdout>,
    pending: &mut Vec<u8>,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    loop {
        if let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            if end > limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "worker frame too large",
                ));
            }
            let mut frame: Vec<_> = pending.drain(..=end).collect();
            frame.pop();
            return Ok(Some(frame));
        }
        if pending.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "worker frame too large",
            ));
        }
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if pending.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete worker frame",
                ))
            };
        }
        let size = available
            .iter()
            .position(|b| *b == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        if pending.len() + size > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "worker frame too large",
            ));
        }
        let complete = available.get(size - 1) == Some(&b'\n');
        pending.extend_from_slice(&available[..size]);
        reader.consume(size);
        if complete {
            pending.pop();
            return Ok(Some(std::mem::take(pending)));
        }
    }
}

async fn worker_local() -> Result<()> {
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut first = String::new();
    stdin
        .read_line(&mut first)
        .await
        .context("missing worker config")?;
    let start: WorkerInput =
        serde_json::from_str(&first).context("invalid worker start message")?;
    let WorkerInput::Start {
        code,
        output_limit,
        heap_limit,
    } = start
    else {
        anyhow::bail!("expected worker start message");
    };
    if code.len() > MAX_CODE_BYTES {
        anyhow::bail!("worker input code too large");
    }
    let (outbound, mut messages) = mpsc::channel::<WorkerMessage>(16);
    let bridge = Arc::new(ExecutionBridge {
        next_id: AtomicU64::new(1),
        outbound,
        pending: Arc::new(StdMutex::new(HashMap::new())),
        pending_slots: Arc::new(Semaphore::new(16)),
    });
    let pending = bridge.pending.clone();
    tokio::spawn(async move {
        loop {
            let mut line = String::new();
            match stdin.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line.len() > MAX_IPC_LINE_BYTES {
                break;
            }
            let Ok(input) = serde_json::from_str::<WorkerInput>(&line) else {
                continue;
            };
            if let WorkerInput::ToolResult { id, .. } = input
                && let Some((sender, _permit)) = pending.lock().unwrap().remove(&id)
            {
                let _ = sender.send(input);
            }
        }
    });
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let writer = stdout.clone();
    tokio::spawn(async move {
        while let Some(message) = messages.recv().await {
            let Ok(mut bytes) = serde_json::to_vec(&message) else {
                break;
            };
            bytes.push(b'\n');
            let mut stdout = writer.lock().await;
            if stdout.write_all(&bytes).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });
    let extension: Extension = gateway_extension::init();
    let create_params =
        deno_core::v8::Isolate::create_params().heap_limits(0, heap_limit.max(16 * 1024 * 1024));
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![extension],
        create_params: Some(create_params),
        ..Default::default()
    });
    runtime.op_state().borrow_mut().put(bridge);
    let code_literal = serde_json::to_string(&code)?;
    let script = format!(
        r#"
      (async () => {{
        const __bridge = Deno.core.ops.op_gateway_call;
        const __AsyncFunction = Object.getPrototypeOf(async function(){{}}).constructor;
        const __pendingCalls = new Set();
        const __call = (name, argumentsValue = {{}}) => {{
          const task = (async () => {{
          if (typeof name !== 'string' || !name.includes('.')) throw Object.assign(new Error('Tool name must be namespace.name.'), {{code:'TOOL_NOT_FOUND'}});
          const raw = await __bridge(JSON.stringify([name, argumentsValue]));
          const response = JSON.parse(raw);
          if (!response.ok) {{ const data = response.error || {{}}; const error = Object.assign(new Error(data.message || 'Tool call failed.'), {{code:data.code || 'UPSTREAM_TOOL_ERROR'}}); if (data.result !== undefined) error.result = data.result; throw error; }}
          const result = response.value;
          if (result && result.isError) {{ const error = Object.assign(new Error('Upstream tool returned an error.'), {{code:'UPSTREAM_TOOL_ERROR', result}}); throw error; }}
          return result;
          }})();
          __pendingCalls.add(task);
          task.then(() => __pendingCalls.delete(task), () => __pendingCalls.delete(task));
          return task;
        }};
        Object.defineProperty(globalThis, 'tools', {{value:Object.freeze({{call:__call}}), writable:false, configurable:false}});
        Object.defineProperty(globalThis, 'console', {{value:Object.freeze({{log(){{}},info(){{}},warn(){{}},error(){{}},debug(){{}}}}), writable:false, configurable:false}});
        globalThis.Deno = undefined;
        try {{
          const fn = new __AsyncFunction({code_literal});
          const value = await fn();
          while (__pendingCalls.size) await Promise.allSettled(Array.from(__pendingCalls));
          let encoded; try {{ encoded = JSON.stringify(value === undefined ? null : value); }} catch (_) {{ return 'err:' + JSON.stringify({{code:'RESULT_NOT_SERIALIZABLE'}}); }}
          if (encoded === undefined) return 'err:' + JSON.stringify({{code:'RESULT_NOT_SERIALIZABLE'}});
          return 'ok:' + encoded;
        }} catch (error) {{
          while (__pendingCalls.size) await Promise.allSettled(Array.from(__pendingCalls));
          const code = typeof error?.code === 'string' ? error.code : 'JAVASCRIPT_ERROR';
          const message = code === 'UPSTREAM_TOOL_ERROR' ? String(error?.message || 'Upstream tool returned an error.').slice(0,240) : '';
          const result = code === 'UPSTREAM_TOOL_ERROR' ? error?.result : undefined;
          return 'err:' + JSON.stringify({{code,message}});
        }}
      }})()
    "#
    );
    let promise = runtime
        .execute_script("gateway.js", script)
        .context("failed to compile JavaScript")?;
    let resolve = runtime.resolve(promise);
    let result_global = runtime
        .with_event_loop_promise(resolve, PollEventLoopOptions::default())
        .await
        .map_err(|_| anyhow!("script execution failed"))?;
    let result_text: String = {
        deno_core::scope!(scope, runtime);
        let local = deno_core::v8::Local::new(scope, result_global);
        serde_v8::from_v8(scope, local).context("worker returned non-string result")?
    };
    let (result, error) = if let Some(encoded) = result_text.strip_prefix("ok:") {
        if encoded.len() > output_limit {
            (
                None,
                Some(ToolError {
                    code: "OUTPUT_TOO_LARGE".into(),
                    message: String::new(),
                    result: None,
                }),
            )
        } else {
            (
                Some(serde_json::from_str(encoded).context("worker result was not JSON")?),
                None,
            )
        }
    } else if let Some(encoded) = result_text.strip_prefix("err:") {
        let data: Value = serde_json::from_str(encoded).context("worker error was not JSON")?;
        let code = data
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("JAVASCRIPT_ERROR")
            .to_owned();
        let message = data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        (
            None,
            Some(ToolError {
                code,
                message,
                result: None,
            }),
        )
    } else {
        (
            None,
            Some(ToolError {
                code: "JAVASCRIPT_ERROR".into(),
                message: String::new(),
                result: None,
            }),
        )
    };
    let message = WorkerMessage::Finished { result, error };
    let mut stdout = stdout.lock().await;
    let mut bytes = serde_json::to_vec(&message)?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}

pub async fn run_worker() -> Result<()> {
    LocalSet::new().run_until(worker_local()).await
}

fn error_envelope(code: &str, message: &str, _internal_result: Option<Value>) -> Value {
    json!({"error":{"code":code,"message":message}})
}
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

async fn persist_start(gateway: &GatewayState, id: &str, code_bytes: usize, revision: &str) {
    let db = gateway.db.lock().await;
    let _ = db.execute("INSERT INTO executions(id,started_at,outcome,code_bytes,revision) VALUES(?1,CURRENT_TIMESTAMP,'queued',?2,?3)", rusqlite::params![id, code_bytes as i64, revision]);
}
async fn persist_call(gateway: &GatewayState, id: &str, call: &ExecutionCallRecord) {
    let db = gateway.db.lock().await;
    let _ = db.execute("INSERT INTO execution_events(execution_id,kind,tool_name,decision,matched_pattern,bytes,error_code,duration_ms) VALUES(?1,'tool_call',?2,?3,?4,?5,?6,?7)", rusqlite::params![id, call.tool_name, call.decision, call.matched_pattern, call.bytes as i64, call.error_code, call.duration_ms as i64]);
}

pub const fn description() -> &'static str {
    EXECUTE_DESCRIPTION
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionContext, ExecutionManager, SandboxBackend, SandboxBackendFactory, ToolError,
    };
    use crate::{config, state::GatewayState};
    use anyhow::{Result, bail};
    use futures::future::BoxFuture;
    use serde_json::{Value, json};
    use std::{path::PathBuf, sync::Arc, time::Duration};

    struct FakeFactory;
    impl SandboxBackendFactory for FakeFactory {
        fn build(&self, provider: &str) -> Result<Box<dyn SandboxBackend>> {
            if provider != "deno" {
                bail!("unexpected provider");
            }
            Ok(Box::new(FakeBackend))
        }
    }

    struct FakeBackend;
    impl SandboxBackend for FakeBackend {
        fn execute(
            &self,
            code: String,
            _output_limit: usize,
            _heap_limit_mb: usize,
            _memory_limit_mb: usize,
            context: Arc<ExecutionContext>,
        ) -> BoxFuture<'static, std::result::Result<Value, ToolError>> {
            Box::pin(async move {
                match code.as_str() {
                    "cancel" => {
                        tokio::select! {
                            _ = context.cancel.cancelled() => Err(ToolError {
                                code: "CANCELLED".into(),
                                message: "Execution was cancelled.".into(),
                                result: None,
                            }),
                            _ = tokio::time::sleep(Duration::from_secs(5)) => Ok(json!("unexpected")),
                        }
                    }
                    "error" => Err(ToolError {
                        code: "JAVASCRIPT_ERROR".into(),
                        message: "script failed".into(),
                        result: None,
                    }),
                    _ => Ok(json!({"value": 42})),
                }
            })
        }
    }

    async fn gateway() -> (Arc<GatewayState>, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("code-mode-mcp-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut config = config::parse(include_str!("../config.yaml")).unwrap();
        config.observability.database = directory.join("gateway.db").to_string_lossy().into_owned();
        let state = Arc::new(
            GatewayState::new(directory.join("config.yaml"), config)
                .await
                .unwrap(),
        );
        (state, directory)
    }

    fn manager() -> Arc<ExecutionManager> {
        Arc::new(ExecutionManager::with_factory(Arc::new(FakeFactory)))
    }

    #[tokio::test]
    async fn fake_backend_result_uses_the_public_json_envelope() {
        let (state, directory) = gateway().await;
        let manager = manager();

        let result = manager.run(state.clone(), "ok".into(), None, None).await;

        assert_eq!(result, json!({"result":{"value":42}}));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn fake_backend_errors_are_redacted_to_the_public_error_envelope() {
        let (state, directory) = gateway().await;
        let manager = manager();

        let result = manager.run(state.clone(), "error".into(), None, None).await;

        assert_eq!(
            result,
            json!({"error":{"code":"JAVASCRIPT_ERROR","message":"script failed"}})
        );
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn running_fake_backend_can_be_cancelled_and_finishes_cleanly() {
        let (state, directory) = gateway().await;
        let manager = manager();
        let run_manager = manager.clone();
        let run_state = state.clone();
        let run = tokio::spawn(async move {
            run_manager
                .run(run_state, "cancel".into(), None, None)
                .await
        });

        let mut execution_id = None;
        for _ in 0..100 {
            if let Some(record) = manager
                .list(10)
                .await
                .into_iter()
                .find(|record| record.status == "running")
            {
                execution_id = Some(record.id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let execution_id = execution_id.expect("execution should start");
        assert!(manager.cancel(&execution_id).await);
        assert_eq!(
            run.await.unwrap(),
            json!({"error":{"code":"CANCELLED","message":"Execution was cancelled."}})
        );
        assert_eq!(
            manager.get(&execution_id).await.unwrap().status,
            "cancelled"
        );

        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
