use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub server: ServerConfig,
    #[serde(default)]
    pub upstreams: BTreeMap<String, UpstreamConfig>,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: String,
    #[serde(default = "default_mcp_path")]
    pub mcp_path: String,
    #[serde(default)]
    pub auth: AuthConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_true")]
    pub client_enabled: bool,
    #[serde(default = "default_true")]
    pub admin_enabled: bool,
    #[serde(default, skip_serializing)]
    pub client_token_env: Option<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)] // Accepted only to read configurations written by older releases.
    pub admin_token_env: Option<String>,
}
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            client_enabled: true,
            admin_enabled: true,
            client_token_env: None,
            admin_token_env: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub description: String,
    pub transport: UpstreamTransport,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpstreamTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    #[serde(default = "default_search_provider")]
    pub provider: String,
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
    #[serde(default = "default_search_limit")]
    pub default_limit: usize,
    #[serde(default = "default_max_limit")]
    pub max_limit: usize,
    #[serde(default = "default_search_bytes")]
    pub max_response_bytes: usize,
}
impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: default_search_provider(),
            options: BTreeMap::new(),
            default_limit: default_search_limit(),
            max_limit: default_max_limit(),
            max_response_bytes: default_search_bytes(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    #[serde(default = "default_sandbox_provider")]
    pub provider: String,
    #[serde(default)]
    pub options: DenoOptions,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_concurrency")]
    pub max_concurrent_executions: usize,
    #[serde(default = "default_queue")]
    pub max_queue_size: usize,
    #[serde(default = "default_tool_calls")]
    pub max_tool_calls: usize,
    #[serde(default = "default_parallel_calls")]
    pub max_parallel_tool_calls: usize,
    #[serde(default = "default_output_bytes")]
    pub max_output_bytes: usize,
}
impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            provider: default_sandbox_provider(),
            options: DenoOptions::default(),
            timeout_ms: default_timeout(),
            max_concurrent_executions: default_concurrency(),
            max_queue_size: default_queue(),
            max_tool_calls: default_tool_calls(),
            max_parallel_tool_calls: default_parallel_calls(),
            max_output_bytes: default_output_bytes(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenoOptions {
    #[serde(default = "default_heap")]
    pub heap_limit_mb: usize,
    #[serde(default = "default_worker_memory")]
    pub worker_memory_limit_mb: usize,
}
impl Default for DenoOptions {
    fn default() -> Self {
        Self {
            heap_limit_mb: default_heap(),
            worker_memory_limit_mb: default_worker_memory(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    #[serde(default = "default_policy")]
    pub default: String,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}
impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            default: default_policy(),
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    #[serde(default = "default_db")]
    pub database: String,
    #[serde(default = "default_retention")]
    pub retention_days: u64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}
impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            database: default_db(),
            retention_days: default_retention(),
            log_level: default_log_level(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_mcp_path() -> String {
    "/mcp".into()
}
fn default_search_provider() -> String {
    "keyword".into()
}
fn default_sandbox_provider() -> String {
    "deno".into()
}
fn default_search_limit() -> usize {
    5
}
fn default_max_limit() -> usize {
    20
}
fn default_search_bytes() -> usize {
    65_536
}
fn default_timeout() -> u64 {
    30_000
}
fn default_concurrency() -> usize {
    4
}
fn default_queue() -> usize {
    32
}
fn default_tool_calls() -> usize {
    50
}
fn default_parallel_calls() -> usize {
    4
}
fn default_output_bytes() -> usize {
    131_072
}
fn default_heap() -> usize {
    128
}
fn default_worker_memory() -> usize {
    256
}
fn default_policy() -> String {
    "allow".into()
}
fn default_db() -> String {
    "./data/gateway.db".into()
}
fn default_retention() -> u64 {
    7
}
fn default_log_level() -> String {
    "info".into()
}

pub fn load(path: &Path) -> anyhow::Result<Config> {
    let metadata = fs::metadata(path).with_context(|| format!("cannot stat {}", path.display()))?;
    if metadata.len() > 1024 * 1024 {
        bail!("configuration file exceeds 1 MiB");
    }
    let yaml =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse(&yaml)
}

pub fn parse(yaml: &str) -> anyhow::Result<Config> {
    if yaml.len() > 1024 * 1024 {
        bail!("configuration file exceeds 1 MiB");
    }
    reject_duplicate_keys(yaml)?;
    let mut config: Config = serde_yaml::from_str(yaml).context("invalid YAML configuration")?;
    for upstream in config.upstreams.values_mut() {
        upstream.description = normalize_description(&upstream.description);
    }
    config.validate()?;
    Ok(config)
}

impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != 1 {
            bail!("unsupported config version {}; expected 1", self.version);
        }
        if self.server.listen.parse::<std::net::SocketAddr>().is_err() {
            bail!("server.listen must be a socket address");
        }
        if !self.server.mcp_path.starts_with('/')
            || self.server.mcp_path.contains('?')
            || self.server.mcp_path.contains('#')
        {
            bail!("server.mcp_path must be an absolute path");
        }
        if self.upstreams.len() > 256 {
            bail!("configuration supports at most 256 upstreams");
        }
        if self.search.provider != "keyword" || !self.search.options.is_empty() {
            bail!("unsupported search provider or options");
        }
        if self.sandbox.provider != "deno" {
            bail!("unsupported sandbox provider: {}", self.sandbox.provider);
        }
        if !matches!(self.policy.default.as_str(), "allow" | "deny") {
            bail!("policy.default must be allow or deny");
        }
        if self.search.default_limit == 0
            || self.search.max_limit == 0
            || self.search.default_limit > self.search.max_limit
            || self.search.max_limit > 1_000
            || self.search.max_response_bytes < 2
            || self.search.max_response_bytes > 1024 * 1024
        {
            bail!("invalid search limits");
        }
        if self.sandbox.timeout_ms == 0
            || self.sandbox.max_concurrent_executions == 0
            || self.sandbox.max_tool_calls == 0
            || self.sandbox.max_parallel_tool_calls == 0
            || self.sandbox.max_output_bytes == 0
        {
            bail!("sandbox limits must be positive");
        }
        if self.sandbox.max_output_bytes > 8 * 1024 * 1024 {
            bail!("sandbox output limit cannot exceed 8 MiB");
        }
        if self.sandbox.options.heap_limit_mb < 16
            || self.sandbox.options.worker_memory_limit_mb <= self.sandbox.options.heap_limit_mb
        {
            bail!("worker memory limit must exceed a heap limit of at least 16 MiB");
        }
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            bail!("Deno sandbox requires process memory monitoring support");
        }
        if !self.observability.log_level.eq_ignore_ascii_case("trace")
            && !self.observability.log_level.eq_ignore_ascii_case("debug")
            && !self.observability.log_level.eq_ignore_ascii_case("info")
            && !self.observability.log_level.eq_ignore_ascii_case("warn")
            && !self.observability.log_level.eq_ignore_ascii_case("error")
        {
            bail!("invalid observability.log_level");
        }
        let listen: std::net::SocketAddr = self.server.listen.parse().expect("validated above");
        if !listen.ip().is_loopback() {
            if !self.server.auth.client_enabled || !self.server.auth.admin_enabled {
                bail!("remote listeners require client and admin authentication enabled");
            }
            if std::env::var("GATEWAY_TLS_TERMINATED").ok().as_deref() != Some("true") {
                bail!("remote listeners require TLS termination and GATEWAY_TLS_TERMINATED=true");
            }
            let hosts: Vec<String> = std::env::var("GATEWAY_ALLOWED_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(str::to_owned)
                .collect();
            if hosts.is_empty()
                || hosts
                    .iter()
                    .any(|host| *host == "*" || host.parse::<http::uri::Authority>().is_err())
            {
                bail!("remote listeners require explicit host names in GATEWAY_ALLOWED_HOSTS");
            }
            let origins: Vec<String> = std::env::var("GATEWAY_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect();
            if origins.is_empty() {
                bail!("remote listeners require explicit GATEWAY_ALLOWED_ORIGINS");
            }
            for origin in origins {
                let parsed = url::Url::parse(&origin)
                    .context("GATEWAY_ALLOWED_ORIGINS must contain valid HTTPS origins")?;
                if parsed.scheme() != "https"
                    || parsed.host_str().is_none()
                    || parsed.path() != "/"
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                    || parsed.username() != ""
                    || parsed.password().is_some()
                {
                    bail!(
                        "GATEWAY_ALLOWED_ORIGINS must contain exact HTTPS origins without paths or credentials"
                    );
                }
            }
        }
        for (name, upstream) in &self.upstreams {
            if name.is_empty()
                || !name.as_bytes()[0].is_ascii_lowercase()
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
            {
                bail!("invalid upstream namespace: {name}");
            }
            if name.contains('.') {
                bail!("upstream namespace cannot contain a dot: {name}");
            }
            let description = normalize_description(&upstream.description);
            if description.is_empty() || description.chars().count() > 512 {
                bail!("upstream {name} description must be 1 to 512 characters");
            }
            match &upstream.transport {
                UpstreamTransport::Stdio { command, env, .. } => {
                    if command.trim().is_empty() {
                        bail!("upstream {name} command is empty");
                    }
                    for (key, value) in env {
                        validate_env_name(name, key)?;
                        if value.contains('\0') {
                            bail!("upstream {name} environment variable {key} contains a NUL byte");
                        }
                    }
                }
                UpstreamTransport::StreamableHttp { url, headers } => {
                    let parsed = url::Url::parse(url).context("invalid upstream URL")?;
                    if !matches!(parsed.scheme(), "http" | "https") {
                        bail!("upstream {name} URL must use http or https");
                    }
                    if !parsed.username().is_empty()
                        || parsed.password().is_some()
                        || parsed.fragment().is_some()
                    {
                        bail!("upstream {name} URL cannot contain credentials or a fragment");
                    }
                    let host = parsed.host_str().unwrap_or_default();
                    let loopback = host.eq_ignore_ascii_case("localhost")
                        || host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|ip| ip.is_loopback());
                    if parsed.scheme() != "https" && !loopback {
                        bail!("upstream {name} must use HTTPS unless it is loopback");
                    }
                    for (header, value) in headers {
                        http::HeaderName::from_bytes(header.as_bytes())
                            .context("invalid upstream HTTP header name")?;
                        if http::HeaderValue::from_str(value).is_err() {
                            bail!("upstream {name} header {header} has an invalid value");
                        }
                    }
                }
            }
        }
        for p in self.policy.allow.iter().chain(&self.policy.deny) {
            validate_pattern(p)?;
        }
        Ok(())
    }
}

fn validate_env_name(scope: &str, name: &str) -> anyhow::Result<()> {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        bail!("{scope} has an empty environment variable name");
    };
    if !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        bail!("{scope} has an invalid environment variable name");
    }
    Ok(())
}

pub fn normalize_description(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_pattern(pattern: &str) -> anyhow::Result<()> {
    if pattern == "*" {
        return Ok(());
    }
    if pattern.is_empty() || pattern.contains('?') || pattern.contains('[') || pattern.contains(']')
    {
        bail!("invalid policy pattern: {pattern}");
    }
    let Some((ns, tool)) = pattern.split_once('.') else {
        bail!("invalid policy pattern: {pattern}");
    };
    if ns.is_empty()
        || tool.is_empty()
        || ns.contains('.')
        || !ns.bytes().all(pattern_char)
        || !tool.bytes().all(|c| pattern_char(c) || c == b'.')
    {
        bail!("invalid policy pattern: {pattern}");
    }
    Ok(())
}

fn pattern_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'*'
}

// Walk mappings before typed deserialization so duplicate keys are rejected even
// in free-form maps such as upstreams, HTTP headers, and provider options.
fn reject_duplicate_keys(yaml: &str) -> anyhow::Result<()> {
    use serde::Deserialize;
    CheckedYaml::deserialize(serde_yaml::Deserializer::from_str(yaml))
        .context("invalid YAML or duplicate mapping key")?;
    Ok(())
}

enum CheckedYaml {
    Scalar,
    Sequence,
    Mapping,
}
impl<'de> Deserialize<'de> for CheckedYaml {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(CheckedVisitor)
    }
}
struct CheckedVisitor;
impl<'de> serde::de::Visitor<'de> for CheckedVisitor {
    type Value = CheckedYaml;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a YAML value with unique string mapping keys")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_string<E: serde::de::Error>(self, _: String) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(CheckedYaml::Scalar)
    }
    fn visit_some<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        CheckedYaml::deserialize(deserializer)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<CheckedYaml>()?.is_some() {}
        Ok(CheckedYaml::Sequence)
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut keys = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate YAML mapping key {key:?}"
                )));
            }
            let _: CheckedYaml = map.next_value()?;
        }
        Ok(CheckedYaml::Mapping)
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn shipped_configuration_parses_and_round_trips() {
        let config = parse(include_str!("../config.yaml")).unwrap();
        let serialized = serde_yaml::to_string(&config).unwrap();
        let restored = parse(&serialized).unwrap();

        assert_eq!(restored.version, 1);
        assert_eq!(restored.server.mcp_path, "/mcp");
        assert_eq!(restored.search.provider, "keyword");
        assert_eq!(restored.sandbox.provider, "deno");
    }

    #[test]
    fn duplicate_yaml_keys_are_rejected() {
        let duplicate = r#"
version: 1
server:
  listen: "127.0.0.1:8080"
  listen: "127.0.0.1:8081"
"#;

        assert!(parse(duplicate).is_err());
    }

    #[test]
    fn legacy_token_references_are_read_but_not_written() {
        let legacy = include_str!("../config.yaml")
            .replace(
                "client_enabled: true",
                "client_token_env: GATEWAY_CLIENT_TOKEN",
            )
            .replace(
                "admin_enabled: true",
                "admin_token_env: GATEWAY_ADMIN_TOKEN",
            );
        let config = parse(&legacy).unwrap();
        assert!(config.server.auth.client_enabled);
        assert!(config.server.auth.admin_enabled);
        let serialized = serde_yaml::to_string(&config).unwrap();
        assert!(!serialized.contains("_token_env"));
    }
}
