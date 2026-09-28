import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { gatewayApi } from '../api/gateway'
import type { ConfigSectionKey, GatewayConfig } from '../config/types'
import { cloneConfig, configsEqual } from '../config/yaml'
import { validateConfig } from '../config/validation'

/**
 * Owns the gateway configuration as structured data.
 *
 * - `saved` is the last revision read from or written to the gateway.
 * - `draft` holds unsaved edits from the configuration page.
 * - `commit` applies a focused change (such as adding an upstream) on top of the latest
 *   revision and saves it immediately, without discarding unrelated draft edits.
 */
export function useGatewayConfig(enabled: boolean, configEpoch: number) {
  const [saved, setSaved] = useState<GatewayConfig | null>(null)
  const [draft, setDraft] = useState<GatewayConfig | null>(null)
  const [revision, setRevision] = useState('')
  const [saving, setSaving] = useState(false)

  const dirty = useMemo(() => Boolean(saved && draft && !configsEqual(saved, draft)), [saved, draft])
  const errors = useMemo(() => (draft ? validateConfig(draft) : {}), [draft])

  const dirtyRef = useRef(dirty)
  useEffect(() => {
    dirtyRef.current = dirty
  })

  const load = useCallback(async () => {
    const latest = await gatewayApi.loadConfig()
    setSaved(latest.config)
    setDraft(cloneConfig(latest.config))
    setRevision(latest.revision)
  }, [])

  // Pick up changes made elsewhere (file reload, another tab) unless the user is mid-edit.
  useEffect(() => {
    if (enabled && !dirtyRef.current) void load().catch(() => undefined)
  }, [enabled, configEpoch, load])

  const updateSection = useCallback(<K extends ConfigSectionKey>(key: K, value: GatewayConfig[K]) => {
    setDraft(current => (current ? { ...current, [key]: value } : current))
  }, [])

  const discard = useCallback(() => {
    if (saved) setDraft(cloneConfig(saved))
  }, [saved])

  const save = useCallback(async () => {
    if (!draft) return
    setSaving(true)
    try {
      const nextRevision = await gatewayApi.saveConfig(draft, revision)
      setSaved(cloneConfig(draft))
      setRevision(nextRevision)
    } finally {
      setSaving(false)
    }
  }, [draft, revision])

  const commit = useCallback(async (change: (config: GatewayConfig) => GatewayConfig) => {
    setSaving(true)
    try {
      const latest = await gatewayApi.loadConfig()
      const next = change(cloneConfig(latest.config))
      const nextRevision = await gatewayApi.saveConfig(next, latest.revision)
      const keepDraft = dirtyRef.current
      setSaved(next)
      setRevision(nextRevision)
      setDraft(current => (keepDraft && current ? { ...current, upstreams: cloneConfig(next).upstreams } : cloneConfig(next)))
    } finally {
      setSaving(false)
    }
  }, [])

  const validateRemotely = useCallback(() => (draft ? gatewayApi.validateConfig(draft) : Promise.resolve(null)), [draft])

  return { saved, draft, revision, dirty, errors, saving, load, updateSection, discard, save, commit, validateRemotely }
}

export type GatewayConfigState = ReturnType<typeof useGatewayConfig>
