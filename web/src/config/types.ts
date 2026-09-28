/**
 * Typed mirror of the gateway `config.yaml` schema (see src/config.rs).
 * The API returns the fully serialized configuration, so every field is present.
 */

export type ValueMap = Record<string, string>

export type StdioTransport = {
  type: 'stdio'
  command: string
  args: string[]
  env: ValueMap
}

export type HttpTransport = {
  type: 'streamable_http'
  url: string
  headers: ValueMap
}

export type UpstreamTransport = StdioTransport | HttpTransport

export type UpstreamConfig = {
  enabled: boolean
  description: string
  transport: UpstreamTransport
}

export type ServerConfig = {
  listen: string
  mcp_path: string
  auth: { client_enabled: boolean; admin_enabled: boolean }
}

export type SearchConfig = {
  provider: string
  options: Record<string, unknown>
  default_limit: number
  max_limit: number
  max_response_bytes: number
}

export type SandboxConfig = {
  provider: string
  options: { heap_limit_mb: number; worker_memory_limit_mb: number }
  timeout_ms: number
  max_concurrent_executions: number
  max_queue_size: number
  max_tool_calls: number
  max_parallel_tool_calls: number
  max_output_bytes: number
}

export type PolicyConfig = {
  default: 'allow' | 'deny'
  allow: string[]
  deny: string[]
}

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error'

export type ObservabilityConfig = {
  database: string
  retention_days: number
  log_level: LogLevel
}

export type GatewayConfig = {
  version: number
  server: ServerConfig
  upstreams: Record<string, UpstreamConfig>
  search: SearchConfig
  sandbox: SandboxConfig
  policy: PolicyConfig
  observability: ObservabilityConfig
}

/** Sections that the configuration page edits as a unit. Upstreams are managed on their own page. */
export type ConfigSectionKey = 'server' | 'search' | 'sandbox' | 'policy' | 'observability'

export const LOG_LEVELS: LogLevel[] = ['trace', 'debug', 'info', 'warn', 'error']
