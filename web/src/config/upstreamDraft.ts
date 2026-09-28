/**
 * Editable form state for one upstream. Maps become ordered rows so the form can hold
 * blank or duplicate rows while the user types; `fromDraft` converts back to config.
 * Both transports keep their fields, so switching transport never discards input.
 */
import type { UpstreamConfig, ValueMap } from './types'

export type KeyValueRow = { key: string; value: string }

export type UpstreamDraft = {
  namespace: string
  description: string
  enabled: boolean
  transport: 'stdio' | 'streamable_http'
  command: string
  args: string[]
  env: KeyValueRow[]
  url: string
  headers: KeyValueRow[]
}

function toRows(map: ValueMap): KeyValueRow[] {
  return Object.entries(map).map(([key, value]) => ({ key, value }))
}

function toMap(rows: KeyValueRow[]): ValueMap {
  return Object.fromEntries(
    rows
      .map(row => ({ key: row.key.trim(), value: row.value.trim() }))
      .filter(row => row.key || row.value)
      .map(row => [row.key, row.value]),
  )
}

export function newDraft(): UpstreamDraft {
  return { namespace: '', description: '', enabled: true, transport: 'stdio', command: '', args: [], env: [], url: '', headers: [] }
}

export function toDraft(namespace: string, upstream: UpstreamConfig): UpstreamDraft {
  const draft = { ...newDraft(), namespace, description: upstream.description, enabled: upstream.enabled }
  const { transport } = upstream
  return transport.type === 'stdio'
    ? { ...draft, transport: 'stdio', command: transport.command, args: [...transport.args], env: toRows(transport.env) }
    : { ...draft, transport: 'streamable_http', url: transport.url, headers: toRows(transport.headers) }
}

export function fromDraft(draft: UpstreamDraft): UpstreamConfig {
  const description = draft.description.trim().replace(/\s+/g, ' ')
  return draft.transport === 'stdio'
    ? {
        enabled: draft.enabled,
        description,
        transport: { type: 'stdio', command: draft.command.trim(), args: draft.args.filter(arg => arg !== ''), env: toMap(draft.env) },
      }
    : {
        enabled: draft.enabled,
        description,
        transport: { type: 'streamable_http', url: draft.url.trim(), headers: toMap(draft.headers) },
      }
}
