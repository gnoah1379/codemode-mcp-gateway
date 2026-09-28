/**
 * Client-side validation that mirrors `Config::validate` in src/config.rs, so the form can point at
 * the exact field before the gateway rejects a save. The gateway remains the source of truth.
 */
import type { GatewayConfig } from './types'
import type { KeyValueRow, UpstreamDraft } from './upstreamDraft'

/** Field path (for example `sandbox.timeout_ms`) mapped to a human-readable message. */
export type ValidationErrors = Record<string, string>

const NAMESPACE = /^[a-z][a-z0-9_-]*$/
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/
const HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/
const PATTERN_SEGMENT = /^[A-Za-z0-9_*-]+$/
const PATTERN_TOOL = /^[A-Za-z0-9_*.-]+$/
const SOCKET_ADDRESS = /^(?:(\d{1,3}(?:\.\d{1,3}){3})|\[([0-9A-Fa-f:.]+)\]):(\d{1,5})$/

const MIB = 1024 * 1024

export function isLoopbackListen(listen: string): boolean {
  const match = SOCKET_ADDRESS.exec(listen.trim())
  if (!match) return false
  return match[1] ? match[1].startsWith('127.') : match[2] === '::1'
}

function isSocketAddress(value: string): boolean {
  const match = SOCKET_ADDRESS.exec(value.trim())
  if (!match) return false
  const port = Number(match[3])
  const octetsValid = !match[1] || match[1].split('.').every(octet => Number(octet) <= 255)
  return octetsValid && port <= 65_535
}

export function patternError(pattern: string): string | null {
  if (pattern === '*') return null
  if (!pattern || /[?[\]]/.test(pattern)) return 'Use namespace.tool with optional * wildcards.'
  const dot = pattern.indexOf('.')
  if (dot <= 0 || dot === pattern.length - 1) return 'Pattern must look like namespace.tool, for example github.*'
  const namespace = pattern.slice(0, dot)
  const tool = pattern.slice(dot + 1)
  if (!PATTERN_SEGMENT.test(namespace) || !PATTERN_TOOL.test(tool)) {
    return 'Only letters, digits, _, -, * and dots in the tool part are allowed.'
  }
  return null
}

export function envNameError(name: string): string | null {
  if (!name) return 'Required.'
  return ENV_NAME.test(name) ? null : 'Use letters, digits and _; must not start with a digit.'
}

function isPositiveInteger(value: number): boolean {
  return Number.isInteger(value) && value > 0
}

export function validateConfig(config: GatewayConfig): ValidationErrors {
  const errors: ValidationErrors = {}
  const { server, search, sandbox, policy, observability } = config

  if (!isSocketAddress(server.listen)) errors['server.listen'] = 'Use host:port, for example 127.0.0.1:8080.'
  if (!server.mcp_path.startsWith('/') || /[?#]/.test(server.mcp_path)) {
    errors['server.mcp_path'] = 'Must be an absolute path such as /mcp.'
  }
  if (isSocketAddress(server.listen) && !isLoopbackListen(server.listen)) {
    if (!server.auth.client_enabled) errors['server.auth.client_enabled'] = 'Required when listening beyond localhost.'
    if (!server.auth.admin_enabled) errors['server.auth.admin_enabled'] = 'Required when listening beyond localhost.'
  }

  if (!isPositiveInteger(search.default_limit)) errors['search.default_limit'] = 'Must be at least 1.'
  if (!isPositiveInteger(search.max_limit) || search.max_limit > 1_000) errors['search.max_limit'] = 'Must be between 1 and 1000.'
  else if (search.default_limit > search.max_limit) errors['search.default_limit'] = 'Cannot exceed the maximum results.'
  if (!Number.isInteger(search.max_response_bytes) || search.max_response_bytes < 2 || search.max_response_bytes > MIB) {
    errors['search.max_response_bytes'] = 'Must be between 2 bytes and 1 MiB.'
  }

  const positiveSandboxFields = ['timeout_ms', 'max_concurrent_executions', 'max_tool_calls', 'max_parallel_tool_calls', 'max_output_bytes'] as const
  for (const field of positiveSandboxFields) {
    if (!isPositiveInteger(sandbox[field])) errors[`sandbox.${field}`] = 'Must be a positive whole number.'
  }
  if (!Number.isInteger(sandbox.max_queue_size) || sandbox.max_queue_size < 0) errors['sandbox.max_queue_size'] = 'Must be zero or more.'
  if (sandbox.max_output_bytes > 8 * MIB) errors['sandbox.max_output_bytes'] = 'Cannot exceed 8 MiB.'
  if (!Number.isInteger(sandbox.options.heap_limit_mb) || sandbox.options.heap_limit_mb < 16) {
    errors['sandbox.options.heap_limit_mb'] = 'Must be at least 16 MiB.'
  }
  if (!Number.isInteger(sandbox.options.worker_memory_limit_mb) || sandbox.options.worker_memory_limit_mb <= sandbox.options.heap_limit_mb) {
    errors['sandbox.options.worker_memory_limit_mb'] = 'Must be larger than the heap limit.'
  }

  policy.allow.forEach((pattern, index) => {
    const error = patternError(pattern)
    if (error) errors[`policy.allow.${index}`] = error
  })
  policy.deny.forEach((pattern, index) => {
    const error = patternError(pattern)
    if (error) errors[`policy.deny.${index}`] = error
  })

  if (!observability.database.trim()) errors['observability.database'] = 'Required.'
  if (!Number.isInteger(observability.retention_days) || observability.retention_days < 0) {
    errors['observability.retention_days'] = 'Must be zero or more days.'
  }

  return errors
}

/**
 * Validates the upstream form. `takenNamespaces` is only passed when creating a new upstream,
 * so an existing entry does not collide with itself.
 */
export function validateUpstreamDraft(draft: UpstreamDraft, takenNamespaces?: string[]): ValidationErrors {
  const errors: ValidationErrors = {}

  if (!NAMESPACE.test(draft.namespace) || draft.namespace.length > 64) {
    errors.namespace = 'Start with a lowercase letter; use a-z, 0-9, _ or - (max 64).'
  } else if (takenNamespaces?.includes(draft.namespace)) {
    errors.namespace = 'This namespace is already in use.'
  }

  const description = draft.description.trim().replace(/\s+/g, ' ')
  if (!description) errors.description = 'Describe what this server does; clients see it when searching.'
  else if (description.length > 512) errors.description = 'Keep the description under 512 characters.'

  if (draft.transport === 'stdio') {
    if (!draft.command.trim()) errors.command = 'Enter the executable to launch.'
    validateRows(draft.env, 'env', envNameError, errors)
  } else {
    const error = urlError(draft.url.trim())
    if (error) errors.url = error
    validateRows(draft.headers, 'headers', header => (HEADER_NAME.test(header) ? null : 'Invalid HTTP header name.'), errors)
  }

  return errors
}

function validateRows(rows: KeyValueRow[], prefix: string, keyError: (key: string) => string | null, errors: ValidationErrors) {
  const seen = new Set<string>()
  rows.forEach((row, index) => {
    const key = row.key.trim()
    const value = row.value.trim()
    if (!key && !value) return
    const error = key ? keyError(key) : 'Required.'
    if (error) errors[`${prefix}.${index}.key`] = error
    else if (seen.has(key)) errors[`${prefix}.${index}.key`] = 'Duplicate name.'
    seen.add(key)
    if (!value) errors[`${prefix}.${index}.value`] = 'Required.'
  })
}

function urlError(value: string): string | null {
  let url: URL
  try {
    url = new URL(value)
  } catch {
    return 'Enter a full URL, for example https://mcp.example.com/mcp.'
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return 'Use http or https.'
  if (url.username || url.password || url.hash) return 'Remove credentials and #fragment from the URL.'
  const host = url.hostname.replace(/^\[|\]$/g, '')
  const loopback = host === 'localhost' || host === '::1' || host.startsWith('127.')
  if (url.protocol === 'http:' && !loopback) return 'Remote servers must use https.'
  return null
}

export function hasErrors(errors: ValidationErrors): boolean {
  return Object.keys(errors).length > 0
}

/** Returns the errors that belong to one section, for example `sandbox`. */
export function errorsFor(errors: ValidationErrors, prefix: string): ValidationErrors {
  return Object.fromEntries(Object.entries(errors).filter(([path]) => path === prefix || path.startsWith(`${prefix}.`)))
}
