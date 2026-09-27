use crate::config;
use anyhow::{Context, Result, bail};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

const SERVICE_NAME: &str = "com.code-mode-mcp.gateway";

pub fn config_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("code-mode-gateway"));
    }
    Ok(home()?.join(".config/code-mode-gateway"))
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path).join("code-mode-gateway"));
    }
    Ok(home()?.join(".local/share/code-mode-gateway"))
}

pub fn default_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.yaml"))
}

fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is required for per-user installation")
}

pub fn init(config_path: &Path) -> Result<()> {
    let config_path = absolute(config_path)?;
    let parent = config_path
        .parent()
        .context("configuration path has no parent")?;
    fs::create_dir_all(parent)?;
    let data = data_dir()?;
    fs::create_dir_all(&data)?;
    if !config_path.exists() {
        let mut candidate = config::parse(include_str!("../config.yaml"))?;
        candidate.observability.database = data.join("gateway.db").to_string_lossy().into_owned();
        write_private(&config_path, serde_yaml::to_string(&candidate)?.as_bytes())?;
        println!("Created {}", config_path.display());
    } else {
        println!("Using existing {}", config_path.display());
    }
    let env_path = parent.join("service.env");
    if !env_path.exists() {
        let client = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let admin = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        write_private(
            &env_path,
            format!("GATEWAY_CLIENT_TOKEN={client}\nGATEWAY_ADMIN_TOKEN={admin}\n").as_bytes(),
        )?;
        println!("Created {} (mode 0600)", env_path.display());
    }
    Ok(())
}

pub fn show_token(kind: &str, config_path: &Path) -> Result<()> {
    let name = match kind {
        "client" => "GATEWAY_CLIENT_TOKEN",
        "admin" => "GATEWAY_ADMIN_TOKEN",
        _ => bail!("token kind must be client or admin"),
    };
    let path = absolute(config_path)?
        .parent()
        .context("configuration path has no parent")?
        .join("service.env");
    let value = parse_env(&path)?
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .with_context(|| format!("{name} is not in {}", path.display()))?;
    println!("{value}");
    Ok(())
}

pub fn service_exec(config_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let binary = env::current_exe()?;
        let env_path = config_path
            .parent()
            .context("configuration path has no parent")?
            .join("service.env");
        let mut command = Command::new(binary);
        command.arg("--config").arg(config_path);
        for (key, value) in parse_env(&env_path)? {
            command.env(key, value);
        }
        Err(command.exec()).context("could not replace service launcher with gateway")
    }
    #[cfg(not(unix))]
    {
        let _ = config_path;
        bail!("background service is supported on macOS and Linux")
    }
}

pub fn install(config_path: &Path) -> Result<()> {
    let config_path = absolute(config_path)?;
    init(&config_path)?;
    let candidate = config::load(&config_path)?;
    let service_env = parse_env(&config_path.parent().unwrap().join("service.env"))?;
    for required in [
        candidate.server.auth.client_token_env.as_deref(),
        candidate.server.auth.admin_token_env.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !service_env.iter().any(|(name, _)| name == required) {
            bail!("service.env is missing configured token variable {required}");
        }
    }
    let binary = env::current_exe()?;
    if cfg!(target_os = "macos") {
        install_launchd(&binary, &config_path)?;
    } else if cfg!(target_os = "linux") {
        install_systemd(&binary, &config_path)?;
    } else {
        bail!("background service is supported on macOS and Linux");
    }
    Ok(())
}

pub fn start() -> Result<()> {
    if cfg!(target_os = "macos") {
        let plist = launchd_path()?;
        run(
            "launchctl",
            &["bootstrap", &launchd_domain()?, &path_str(&plist)?],
        )?;
    } else {
        run("systemctl", &["--user", "start", SERVICE_NAME])?;
    }
    Ok(())
}

pub fn stop() -> Result<()> {
    if cfg!(target_os = "macos") {
        run(
            "launchctl",
            &[
                "bootout",
                &format!("{}/{}", launchd_domain()?, SERVICE_NAME),
            ],
        )?;
    } else {
        run("systemctl", &["--user", "stop", SERVICE_NAME])?;
    }
    Ok(())
}

pub fn status() -> Result<()> {
    if cfg!(target_os = "macos") {
        run(
            "launchctl",
            &["print", &format!("{}/{}", launchd_domain()?, SERVICE_NAME)],
        )?;
    } else {
        run(
            "systemctl",
            &["--user", "status", SERVICE_NAME, "--no-pager"],
        )?;
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if cfg!(target_os = "macos") {
        let _ = stop();
        let path = launchd_path()?;
        if path.exists() {
            fs::remove_file(path)?;
        }
    } else if cfg!(target_os = "linux") {
        let _ = run("systemctl", &["--user", "disable", "--now", SERVICE_NAME]);
        let path = systemd_path()?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        run("systemctl", &["--user", "daemon-reload"])?;
    } else {
        bail!("background service is supported on macOS and Linux");
    }
    println!("Service removed. Configuration and data were preserved.");
    Ok(())
}

fn install_launchd(binary: &Path, config_path: &Path) -> Result<()> {
    let path = launchd_path()?;
    fs::create_dir_all(path.parent().unwrap())?;
    let data = data_dir()?;
    fs::create_dir_all(&data)?;
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{SERVICE_NAME}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>service-run</string><string>--config</string><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>StandardOutPath</key><string>{}</string>\n<key>StandardErrorPath</key><string>{}</string>\n</dict></plist>\n",
        xml(&binary.to_string_lossy()),
        xml(&config_path.to_string_lossy()),
        xml(&data.join("gateway.log").to_string_lossy()),
        xml(&data.join("gateway.err.log").to_string_lossy())
    );
    let _ = stop();
    fs::write(&path, plist)?;
    start()?;
    println!("Installed launchd service at {}", path.display());
    Ok(())
}

fn install_systemd(binary: &Path, config_path: &Path) -> Result<()> {
    let path = systemd_path()?;
    fs::create_dir_all(path.parent().unwrap())?;
    let unit = format!(
        "[Unit]\nDescription=Code Mode MCP Gateway\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} service-run --config {}\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(binary)?,
        systemd_quote(config_path)?
    );
    fs::write(&path, unit)?;
    run("systemctl", &["--user", "daemon-reload"])?;
    run("systemctl", &["--user", "enable", "--now", SERVICE_NAME])?;
    println!("Installed systemd user service at {}", path.display());
    Ok(())
}

fn launchd_path() -> Result<PathBuf> {
    Ok(home()?
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_NAME}.plist")))
}

fn systemd_path() -> Result<PathBuf> {
    Ok(config_dir()?
        .parent()
        .unwrap()
        .join("systemd/user")
        .join(format!("{SERVICE_NAME}.service")))
}

fn launchd_domain() -> Result<String> {
    let output = Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        bail!("could not determine user ID");
    }
    Ok(format!("gui/{}", String::from_utf8(output.stdout)?.trim()))
}

fn parse_env(path: &Path) -> Result<Vec<(String, String)>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!("service.env must be readable only by its owner (chmod 600)");
        }
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("cannot read service credentials {}", path.display()))?;
    let mut values = Vec::new();
    for line in content.lines() {
        let (name, value) = line.split_once('=').context("invalid service.env line")?;
        let mut characters = name.bytes();
        let valid_name = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
            && characters.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !valid_name || value.is_empty() || values.iter().any(|(key, _)| key == name) {
            bail!("invalid service credential");
        }
        values.push((name.to_owned(), value.to_owned()));
    }
    for required in ["GATEWAY_CLIENT_TOKEN", "GATEWAY_ADMIN_TOKEN"] {
        if values.iter().filter(|(name, _)| name == required).count() != 1 {
            bail!("service.env must contain exactly one {required}");
        }
    }
    Ok(values)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn path_str(path: &Path) -> Result<String> {
    Ok(path.to_str().context("path is not valid UTF-8")?.to_owned())
}

fn systemd_quote(path: &Path) -> Result<String> {
    let value = path_str(path)?;
    if value.contains(['\n', '\r']) {
        bail!("service path contains a newline");
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("could not run {program}"))?;
    if !status.success() {
        bail!("{program} failed with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_env, systemd_quote, write_private, xml};
    use std::{fs, path::Path};

    #[test]
    fn service_environment_accepts_upstream_secrets_and_rejects_duplicates() {
        let directory =
            std::env::temp_dir().join(format!("gateway-service-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("service.env");
        write_private(&path, b"GATEWAY_CLIENT_TOKEN=client\nGATEWAY_ADMIN_TOKEN=admin\nUPSTREAM_AUTH=Bearer example\n").unwrap();
        let values = parse_env(&path).unwrap();
        assert!(
            values
                .iter()
                .any(|(name, value)| name == "UPSTREAM_AUTH" && value == "Bearer example")
        );
        fs::write(
            &path,
            "GATEWAY_CLIENT_TOKEN=a\nGATEWAY_CLIENT_TOKEN=b\nGATEWAY_ADMIN_TOKEN=c\n",
        )
        .unwrap();
        assert!(parse_env(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn service_template_paths_are_escaped() {
        assert_eq!(
            systemd_quote(Path::new("/tmp/a b%z")).unwrap(),
            "\"/tmp/a b%%z\""
        );
        assert_eq!(xml("a&<b>"), "a&amp;&lt;b&gt;");
    }
}
