use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

#[tokio::test]
async fn reference_client_lists_tools_executes_json_and_recovers_from_timeout() {
    let directory = std::env::temp_dir().join(format!(
        "code-mode-mcp-integration-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_path = directory.join("config.yaml");
    let yaml = include_str!("../config.yaml").replace(
        "./data/gateway.db",
        &directory.join("gateway.db").to_string_lossy(),
    );
    std::fs::write(&config_path, yaml).unwrap();

    let executable = env!("CARGO_BIN_EXE_codemode");
    let mut command = Command::new(executable);
    command.args(["--config", config_path.to_str().unwrap(), "--stdio"]);
    let (transport, _stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::null())
        .spawn()
        .expect("gateway process should start");
    let client = ().serve(transport).await.expect("MCP initialize should succeed");

    let listed = client
        .list_tools(None)
        .await
        .expect("tools/list should succeed");
    let names: Vec<_> = listed.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, ["tools_search", "tools_execute"]);

    let called = tokio::time::timeout(
        Duration::from_secs(15),
        client.call_tool(
            CallToolRequestParams::new("tools_execute").with_arguments(
                json!({"code":"return { answer: 42 };"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .expect("tools_execute should return")
    .expect("tools_execute should succeed");
    let called: Value = serde_json::to_value(called).unwrap();
    let payload: Value =
        serde_json::from_str(called["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload, json!({"result":{"answer":42}}));

    let timed_out = client
        .call_tool(
            CallToolRequestParams::new("tools_execute").with_arguments(
                json!({"code":"while (true) {}", "timeout_ms":250})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .expect("timeout should be returned as a tool result");
    let timed_out: Value = serde_json::to_value(timed_out).unwrap();
    assert!(timed_out["isError"].as_bool().unwrap());
    let timeout_payload: Value =
        serde_json::from_str(timed_out["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(timeout_payload["error"]["code"], "EXECUTION_TIMEOUT");

    let recovered = client
        .call_tool(
            CallToolRequestParams::new("tools_execute")
                .with_arguments(json!({"code":"return 7;"}).as_object().unwrap().clone()),
        )
        .await
        .expect("gateway should accept a call after worker timeout");
    let recovered: Value = serde_json::to_value(recovered).unwrap();
    let recovery_payload: Value =
        serde_json::from_str(recovered["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(recovery_payload, json!({"result":7}));

    let _ = client.cancel().await;
    std::fs::remove_dir_all(directory).unwrap();
}
