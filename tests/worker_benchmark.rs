use futures::future::join_all;
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::process::Command;

#[tokio::test]
#[ignore = "manual benchmark: run with --ignored --nocapture"]
async fn benchmark_worker_startup_rss_and_concurrency() {
    let directory =
        std::env::temp_dir().join(format!("code-mode-mcp-benchmark-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_path = directory.join("config.yaml");
    let yaml = include_str!("../config.yaml").replace(
        "./data/gateway.db",
        &directory.join("gateway.db").to_string_lossy(),
    );
    std::fs::write(&config_path, yaml).unwrap();

    let executable = env!("CARGO_BIN_EXE_code-mode-mcp-server");
    let mut command = Command::new(executable);
    command.args(["--config", config_path.to_str().unwrap(), "--stdio"]);
    let (transport, _stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let gateway_pid = transport.id().map(Pid::from_u32);
    let client = ().serve(transport).await.unwrap();

    let peak_rss = Arc::new(AtomicU64::new(0));
    let sample_peak = peak_rss.clone();
    let sampler = tokio::spawn(async move {
        let mut system = System::new();
        loop {
            system.refresh_processes(ProcessesToUpdate::All, true);
            let tree_rss = system
                .processes()
                .iter()
                .filter(|(pid, process)| {
                    gateway_pid.is_some_and(|gateway| {
                        **pid == gateway || process.parent() == Some(gateway)
                    })
                })
                .map(|(_, process)| process.memory())
                .sum();
            sample_peak.fetch_max(tree_rss, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    let mut startup_micros = Vec::new();
    for index in 0..10 {
        let started = Instant::now();
        let result = client
            .call_tool(
                CallToolRequestParams::new("tools_execute").with_arguments(
                    json!({"code":format!("return {index};")})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        let result: Value = serde_json::to_value(result).unwrap();
        assert!(!result["isError"].as_bool().unwrap_or(false));
        startup_micros.push(started.elapsed().as_micros() as u64);
    }
    startup_micros.sort_unstable();
    let mean = startup_micros.iter().sum::<u64>() / startup_micros.len() as u64;
    let p50 = startup_micros[startup_micros.len() / 2];
    let p95 = startup_micros[(startup_micros.len() * 95 / 100).min(startup_micros.len() - 1)];
    println!("worker_startup samples=10 mean_us={mean} p50_us={p50} p95_us={p95}");

    let started = Instant::now();
    let results = join_all((0..4).map(|index| {
        client.call_tool(
            CallToolRequestParams::new("tools_execute").with_arguments(
                json!({"code":format!("return {index};")})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
    }))
    .await;
    for result in results {
        let value: Value = serde_json::to_value(result.unwrap()).unwrap();
        assert!(!value["isError"].as_bool().unwrap_or(false));
    }
    println!(
        "concurrent_executions=4 wall_ms={} peak_process_tree_rss_mb={:.1}",
        started.elapsed().as_millis(),
        peak_rss.load(Ordering::Relaxed) as f64 / 1_048_576.0
    );

    sampler.abort();
    let _ = client.cancel().await;
    std::fs::remove_dir_all(directory).unwrap();
}
