use crate::{api, config, execution, mcp, service, state};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::{env, fs, path::PathBuf, process::Command, sync::Arc};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "code-mode-mcp-server",
    version,
    about = "Code Mode MCP Gateway"
)]
pub struct Cli {
    /// YAML configuration path
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Legacy stdio flag; prefer the stdio command
    #[arg(long, hide = true)]
    pub stdio: bool,
    #[command(subcommand)]
    pub command: Option<GatewayCommand>,
}

#[derive(Subcommand)]
pub enum GatewayCommand {
    /// Create per-user configuration and service credentials
    Init,
    /// Run the HTTP gateway in the foreground
    Serve,
    /// Run the MCP stdio gateway in the foreground
    Stdio,
    /// Manage the per-user background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Print one generated service token
    Token { kind: TokenKind },
    /// Install the latest binary from GitHub Releases
    Update {
        /// GitHub repository in owner/repo form; defaults to the installed release repository
        #[arg(long)]
        repo: Option<String>,
    },
    #[command(hide = true)]
    ServiceRun,
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install and start a launchd/systemd user service
    Install,
    Start,
    Stop,
    Status,
    /// Remove the service while preserving configuration and data
    Uninstall,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum TokenKind {
    Client,
    Admin,
}

impl TokenKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Admin => "admin",
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let config_path = match cli.config {
        Some(path) => path,
        None => match cli.command.as_ref() {
            Some(GatewayCommand::Serve | GatewayCommand::Stdio) | None
                if PathBuf::from("config.yaml").exists() =>
            {
                PathBuf::from("config.yaml")
            }
            _ => service::default_config_path()?,
        },
    };
    match cli.command {
        Some(GatewayCommand::Init) => service::init(&config_path),
        Some(GatewayCommand::Service { action }) => match action {
            ServiceAction::Install => service::install(&config_path),
            ServiceAction::Start => service::start(),
            ServiceAction::Stop => service::stop(),
            ServiceAction::Status => service::status(),
            ServiceAction::Uninstall => service::uninstall(),
        },
        Some(GatewayCommand::Token { kind }) => service::show_token(kind.as_str(), &config_path),
        Some(GatewayCommand::Update { repo }) => update(repo),
        Some(GatewayCommand::ServiceRun) => service::service_exec(&config_path),
        Some(GatewayCommand::Stdio) => serve(config_path, true),
        Some(GatewayCommand::Serve) | None => serve(config_path, cli.stdio),
    }
}

fn serve(config_path: PathBuf, stdio: bool) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let loaded = config::load(&config_path)
            .context("failed to load config; run `code-mode-mcp-server init` first")?;
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(loaded.observability.log_level.clone())),
            )
            .json()
            .init();
        let gateway = Arc::new(state::GatewayState::new(config_path, loaded).await?);
        if stdio {
            mcp::serve_stdio(gateway).await
        } else {
            api::serve(gateway).await
        }
    })
}

pub fn worker() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(execution::run_worker())
}

fn update(repo: Option<String>) -> Result<()> {
    let repo = match repo {
        Some(repo) => repo,
        None => fs::read_to_string(service::config_dir()?.join("release-repo"))
            .context("release repository is unknown; pass --repo owner/repo")?
            .trim()
            .to_owned(),
    };
    if !valid_repo(&repo) {
        bail!("repository must be owner/repo with ASCII letters, digits, '-', '_' or '.'");
    }
    let script = env::temp_dir().join(format!("gateway-install-{}.sh", uuid::Uuid::new_v4()));
    let url = format!("https://github.com/{repo}/releases/latest/download/install.sh");
    let download = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&script)
        .arg(&url)
        .status()
        .context("could not download release installer")?;
    if !download.success() {
        bail!("could not download release installer from {url}");
    }
    let status = Command::new("sh")
        .arg(&script)
        .arg("--repo")
        .arg(&repo)
        .arg("--update")
        .status()
        .context("could not run release installer")?;
    let _ = fs::remove_file(script);
    if !status.success() {
        bail!("release installer failed with status {status}");
    }
    Ok(())
}

fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !matches!(owner, "" | "." | "..")
        && !matches!(name, "" | "." | "..")
        && !name.contains('/')
        && owner
            .bytes()
            .chain(name.bytes())
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::valid_repo;

    #[test]
    fn repository_name_cannot_add_a_url_or_shell_expression() {
        assert!(valid_repo("example/code-mode-mcp-server"));
        assert!(!valid_repo("https://github.com/example/repo"));
        assert!(!valid_repo("example/repo;rm"));
        assert!(!valid_repo("example/../repo"));
        assert!(!valid_repo("example/.."));
    }
}
