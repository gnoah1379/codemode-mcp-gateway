/** Response shapes returned by the gateway HTTP API. */

export type UpstreamStatus = {
  name: string
  description: string
  transport: 'stdio' | 'streamable_http'
  enabled: boolean
  available: boolean
  tool_count: number
  last_error?: string | null
}

export type Tool = {
  namespace: string
  name: string
  description: string
  inputSchema: unknown
  outputSchema?: unknown
  available: boolean
  allowed: boolean
  matchedPattern?: string | null
}

export type ToolCall = {
  toolName: string
  decision: string
  matchedPattern?: string | null
  durationMs: number
  bytes: number
  errorCode?: string | null
}

export type ExecutionStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'timed_out' | 'cancelled'

export type Execution = {
  id: string
  status: ExecutionStatus | string
  startedAt: number
  finishedAt?: number | null
  durationMs?: number | null
  codeBytes: number
  outputBytes: number
  errorCode?: string | null
  revision: string
  calls: ToolCall[]
}

export type Metrics = {
  executions: {
    total: number
    succeeded: number
    failed: number
    timedOut: number
    cancelled: number
    active: number
    averageDurationMs: number
    outputBytes: number
  }
  toolCalls: number
  tools: { discovered: number; available: number }
  upstreams: { total: number; healthy: number }
}

export type PolicyDecision = { allowed: boolean; matchedPattern?: string | null }

export type ConfigValidation = { valid: boolean; degradedUpstreams: string[] }

export const EMPTY_METRICS: Metrics = {
  executions: { total: 0, succeeded: 0, failed: 0, timedOut: 0, cancelled: 0, active: 0, averageDurationMs: 0, outputBytes: 0 },
  toolCalls: 0,
  tools: { discovered: 0, available: 0 },
  upstreams: { total: 0, healthy: 0 },
}

export function isActiveExecution(execution: Execution): boolean {
  return execution.status === 'queued' || execution.status === 'running'
}
