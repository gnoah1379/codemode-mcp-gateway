/** Typed wrappers for every gateway endpoint used by the console. */
import { getJson, postJson, request } from './client'
import type { ConfigValidation, Execution, Metrics, PolicyDecision, Tool, UpstreamStatus } from './types'
import type { GatewayConfig } from '../config/types'
import { parseConfig, toYaml } from '../config/yaml'

const BASE = '/api/v1'
const path = (segment: string) => `${BASE}${segment}`
const id = encodeURIComponent

export type LoadedConfig = { config: GatewayConfig; revision: string }

export const gatewayApi = {
  signIn: (username: string, password: string) =>
    request(path('/session'), { method: 'POST', body: JSON.stringify({ username, password }) }),
  signOut: () => request(path('/session'), { method: 'DELETE' }),

  metrics: () => getJson<Metrics>(path('/metrics')),
  upstreams: () => getJson<{ upstreams: UpstreamStatus[] }>(path('/upstreams')).then(body => body.upstreams),
  testUpstream: (name: string) => postJson<{ available?: boolean; toolCount?: number }>(path(`/upstreams/${id(name)}/test`)),
  refreshUpstream: (name: string) => postJson<unknown>(path(`/upstreams/${id(name)}/refresh`)),

  tools: (query: string, page: number, pageSize: number) =>
    getJson<{ items: Tool[]; total: number }>(path(`/tools?query=${id(query)}&page=${page}&page_size=${pageSize}`)),
  catalogPreview: () => getJson<{ description: string }>(path('/catalog-preview')).then(body => body.description),
  evaluatePolicy: (name: string) => postJson<PolicyDecision>(path('/policy/evaluate'), { name }),

  executions: (limit = 100) => getJson<{ items: Execution[] }>(path(`/executions?limit=${limit}`)).then(body => body.items),
  execution: (executionId: string) => getJson<Execution>(path(`/executions/${id(executionId)}`)),
  cancelExecution: (executionId: string) => postJson<unknown>(path(`/executions/${id(executionId)}/cancel`)),

  async loadConfig(): Promise<LoadedConfig> {
    const response = await request(path('/config'))
    const revision = response.headers.get('etag')
    if (!revision) throw new Error('Gateway did not return a configuration revision.')
    return { config: parseConfig(await response.text()), revision }
  },

  /** Saves with optimistic concurrency; the gateway rejects the write if `revision` is stale. */
  async saveConfig(config: GatewayConfig, revision: string): Promise<string> {
    const response = await request(path('/config'), {
      method: 'PUT',
      headers: { 'content-type': 'application/yaml', 'if-match': revision },
      body: toYaml(config),
    })
    const body = (await response.json()) as { revision?: string }
    return body.revision ?? response.headers.get('etag') ?? revision
  },

  async validateConfig(config: GatewayConfig): Promise<ConfigValidation> {
    const response = await request(path('/config/validate'), {
      method: 'POST',
      headers: { 'content-type': 'application/yaml' },
      body: toYaml(config),
    })
    return (await response.json()) as ConfigValidation
  },

  reloadConfigFile: () => postJson<unknown>(path('/config/reload')),
}

export const GATEWAY_EVENTS_URL = path('/events')
