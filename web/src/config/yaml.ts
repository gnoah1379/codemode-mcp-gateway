import { parse, stringify } from 'yaml'
import type { GatewayConfig, UpstreamConfig } from './types'

/** Fills optional collections so form components can rely on them being present. */
function normalizeUpstream(upstream: UpstreamConfig): UpstreamConfig {
  const transport = upstream.transport.type === 'stdio'
    ? { ...upstream.transport, args: upstream.transport.args ?? [], env: upstream.transport.env ?? {} }
    : { ...upstream.transport, headers: upstream.transport.headers ?? {} }
  return { ...upstream, enabled: upstream.enabled ?? true, transport }
}

export function parseConfig(yaml: string): GatewayConfig {
  const config = parse(yaml) as GatewayConfig
  const upstreams = Object.fromEntries(
    Object.entries(config.upstreams ?? {}).map(([name, upstream]) => [name, normalizeUpstream(upstream)]),
  )
  return {
    ...config,
    upstreams,
    search: { ...config.search, options: config.search.options ?? {} },
    policy: { ...config.policy, allow: config.policy.allow ?? [], deny: config.policy.deny ?? [] },
  }
}

export function toYaml(config: GatewayConfig): string {
  return stringify(config)
}

export function cloneConfig(config: GatewayConfig): GatewayConfig {
  return structuredClone(config)
}

export function configsEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right)
}
