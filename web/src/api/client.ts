/** Low-level HTTP helpers shared by every gateway API call. */

export class ApiError extends Error {
  readonly status: number
  readonly code?: string

  constructor(message: string, status: number, code?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS'])
const CSRF_COOKIE = 'gateway_csrf='

function csrfToken(): string | undefined {
  const entry = document.cookie.split('; ').find(item => item.startsWith(CSRF_COOKIE))
  return entry ? decodeURIComponent(entry.slice(CSRF_COOKIE.length)) : undefined
}

async function toApiError(response: Response): Promise<ApiError> {
  let message = `Request failed (${response.status})`
  let code: string | undefined
  try {
    const body = (await response.json()) as { error?: { code?: string; message?: string } }
    message = body.error?.message ?? message
    code = body.error?.code
  } catch {
    // The body is not JSON; keep the generic message.
  }
  return new ApiError(message, response.status, code)
}

/** Sends a same-origin request with the CSRF header on mutating methods and throws on non-2xx. */
export async function request(path: string, init: RequestInit = {}): Promise<Response> {
  const method = (init.method ?? 'GET').toUpperCase()
  const headers = new Headers(init.headers)
  if (init.body && !headers.has('content-type')) headers.set('content-type', 'application/json')
  if (!SAFE_METHODS.has(method)) {
    const token = csrfToken()
    if (token) headers.set('x-csrf-token', token)
  }
  const response = await fetch(path, { ...init, method, headers, credentials: 'same-origin' })
  if (!response.ok) throw await toApiError(response)
  return response
}

export async function getJson<T>(path: string): Promise<T> {
  const response = await request(path)
  return (await response.json()) as T
}

export async function postJson<T = void>(path: string, body: unknown = {}): Promise<T> {
  const response = await request(path, { method: 'POST', body: JSON.stringify(body) })
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}

export function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
