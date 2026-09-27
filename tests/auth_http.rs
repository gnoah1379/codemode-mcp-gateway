use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn status(port: u16, method: &str, path: &str) -> Option<u16> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    response.split_whitespace().nth(1)?.parse().ok()
}

#[test]
fn configured_but_missing_tokens_do_not_disable_authentication() {
    let directory =
        std::env::temp_dir().join(format!("gateway-auth-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_path = directory.join("config.yaml");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let yaml = include_str!("../config.yaml")
        .replace("127.0.0.1:8080", &format!("127.0.0.1:{port}"))
        .replace(
            "./data/gateway.db",
            &directory.join("gateway.db").to_string_lossy(),
        );
    std::fs::write(&config_path, yaml).unwrap();

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_code-mode-mcp-server"));
    let child = Command::new(binary)
        .args(["--config", config_path.to_str().unwrap(), "serve"])
        .env_remove("GATEWAY_CLIENT_TOKEN")
        .env_remove("GATEWAY_ADMIN_TOKEN")
        .current_dir(&directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    struct KillOnDrop(std::process::Child);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let child = KillOnDrop(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while status(port, "GET", "/health/live") != Some(200) {
        assert!(Instant::now() < deadline, "gateway did not start");
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(status(port, "GET", "/api/v1/upstreams"), Some(401));
    assert_eq!(status(port, "POST", "/api/v1/session"), Some(401));
    assert_eq!(status(port, "POST", "/mcp"), Some(503));
    drop(child);
    std::fs::remove_dir_all(directory).unwrap();
}
