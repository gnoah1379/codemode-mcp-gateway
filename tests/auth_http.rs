use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use rusqlite::Connection;
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

fn post_login(port: u16, username: &str, password: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let body = serde_json::json!({"username":username,"password":password}).to_string();
    write!(stream, "POST /api/v1/session HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn request_with_headers(port: u16, method: &str, path: &str, headers: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\n{headers}Connection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn mcp_status_with_key(port: u16, key: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(stream, "POST /mcp HTTP/1.1\r\nHost: localhost:{port}\r\nAuthorization: Bearer {key}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn config_can_disable_admin_login_while_client_key_remains_required() {
    let directory =
        std::env::temp_dir().join(format!("gateway-auth-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_path = directory.join("config.yaml");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let yaml = include_str!("../config.yaml")
        .replace("admin_enabled: true", "admin_enabled: false")
        .replace("127.0.0.1:8080", &format!("127.0.0.1:{port}"))
        .replace(
            "./data/gateway.db",
            &directory.join("gateway.db").to_string_lossy(),
        );
    std::fs::write(&config_path, yaml).unwrap();

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_codemode"));
    let child = Command::new(binary)
        .args(["--config", config_path.to_str().unwrap(), "serve"])
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
    assert_eq!(status(port, "GET", "/api/v1/upstreams"), Some(200));
    assert_eq!(status(port, "POST", "/api/v1/session"), Some(200));
    assert_eq!(status(port, "POST", "/mcp"), Some(401));
    let key = Command::new(env!("CARGO_BIN_EXE_codemode"))
        .args(["--config", config_path.to_str().unwrap(), "token", "client"])
        .output()
        .unwrap();
    assert!(key.status.success());
    let key = String::from_utf8(key.stdout).unwrap();
    assert_eq!(key.trim().len(), 64);
    assert_ne!(mcp_status_with_key(port, key.trim()), 401);
    let disabled = std::fs::read_to_string(&config_path)
        .unwrap()
        .replace("client_enabled: true", "client_enabled: false");
    std::fs::write(&config_path, disabled).unwrap();
    assert_eq!(status(port, "POST", "/api/v1/config/reload"), Some(200));
    assert!(!matches!(status(port, "POST", "/mcp"), Some(401 | 503)));
    drop(child);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn admin_login_uses_database_password() {
    let directory =
        std::env::temp_dir().join(format!("gateway-admin-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_path = directory.join("config.yaml");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let db_path = directory.join("gateway.db");
    let yaml = include_str!("../config.yaml")
        .replace("127.0.0.1:8080", &format!("127.0.0.1:{port}"))
        .replace("./data/gateway.db", &db_path.to_string_lossy());
    std::fs::write(&config_path, yaml).unwrap();
    let db = Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE credentials(name TEXT PRIMARY KEY,value TEXT NOT NULL)")
        .unwrap();
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).unwrap();
    let hash = Argon2::default()
        .hash_password(b"correct password 123", &salt)
        .unwrap()
        .to_string();
    db.execute(
        "INSERT INTO credentials VALUES('admin_username','admin')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO credentials VALUES('admin_password_hash',?1)",
        [hash],
    )
    .unwrap();
    drop(db);
    let child = Command::new(env!("CARGO_BIN_EXE_codemode"))
        .args(["--config", config_path.to_str().unwrap(), "serve"])
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
    assert!(post_login(port, "admin", "wrong password").starts_with("HTTP/1.1 401"));
    let login = post_login(port, "admin", "correct password 123");
    assert!(login.starts_with("HTTP/1.1 200"));
    let cookies: Vec<_> = login
        .lines()
        .filter_map(|line| line.strip_prefix("set-cookie: "))
        .collect();
    let session = cookies
        .iter()
        .find(|line| line.starts_with("gateway_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let csrf_cookie = cookies
        .iter()
        .find(|line| line.starts_with("gateway_csrf="))
        .unwrap();
    assert!(csrf_cookie.contains("Path=/;"));
    let csrf = csrf_cookie.split(';').next().unwrap();
    let csrf_value = csrf.strip_prefix("gateway_csrf=").unwrap();
    let headers = format!("Cookie: {session}; {csrf}\r\n");
    assert_eq!(
        request_with_headers(port, "GET", "/api/v1/upstreams", &headers),
        200
    );
    let headers = format!("{headers}x-csrf-token: {csrf_value}\r\n");
    assert_eq!(
        request_with_headers(port, "DELETE", "/api/v1/session", &headers),
        200
    );
    let login = post_login(port, "admin", "correct password 123");
    let session = login
        .lines()
        .filter_map(|line| line.strip_prefix("set-cookie: "))
        .find(|line| line.starts_with("gateway_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let db = Connection::open(&db_path).unwrap();
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).unwrap();
    let replacement = Argon2::default()
        .hash_password(b"replacement password 123", &salt)
        .unwrap()
        .to_string();
    db.execute(
        "UPDATE credentials SET value=?1 WHERE name='admin_password_hash'",
        [replacement],
    )
    .unwrap();
    let headers = format!("Cookie: {session}\r\n");
    assert_eq!(
        request_with_headers(port, "GET", "/api/v1/upstreams", &headers),
        401
    );
    drop(child);
    std::fs::remove_dir_all(directory).unwrap();
}
