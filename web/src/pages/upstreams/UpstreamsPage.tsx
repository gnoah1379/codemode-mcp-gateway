import { useState } from 'react'
import { GitBranch, Plus } from 'lucide-react'
import { gatewayApi } from '../../api/gateway'
import { errorMessage } from '../../api/client'
import type { UpstreamStatus } from '../../api/types'
import { Button, Card, CardHeader, ConfirmDialog, EmptyState, PageHeader } from '../../components/ui'
import { fromDraft, newDraft, toDraft, type UpstreamDraft } from '../../config/upstreamDraft'
import type { GatewayConfigState } from '../../hooks/useGatewayConfig'
import { useToast } from '../../hooks/useToast'
import { plural } from '../../lib/format'
import { UpstreamEditorDialog } from './UpstreamEditorDialog'
import { UpstreamTable } from './UpstreamTable'

type UpstreamsPageProps = {
  upstreams: UpstreamStatus[]
  config: GatewayConfigState
  onDataChanged: () => Promise<void>
}

type Editor = { mode: 'create' | 'edit'; draft: UpstreamDraft }

export function UpstreamsPage({ upstreams, config, onDataChanged }: UpstreamsPageProps) {
  const notify = useToast()
  const [editor, setEditor] = useState<Editor | null>(null)
  const [pendingDelete, setPendingDelete] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const configured = config.saved?.upstreams ?? {}
  const namespaces = Object.keys(configured)

  /** Runs one upstream action with shared busy state and error reporting. */
  const run = async (action: () => Promise<string | void>): Promise<boolean> => {
    setBusy(true)
    try {
      const message = await action()
      await onDataChanged().catch(() => undefined)
      if (message) notify('success', message)
      return true
    } catch (error) {
      notify('error', errorMessage(error, 'The action failed.'))
      return false
    } finally {
      setBusy(false)
    }
  }

  const openEditor = (name: string) => {
    const upstream = configured[name]
    if (!upstream) return notify('error', `${name} is not in the loaded configuration; reload and try again.`)
    setEditor({ mode: 'edit', draft: toDraft(name, upstream) })
  }

  const saveEditor = async (draft: UpstreamDraft) => {
    const created = editor?.mode === 'create'
    const saved = await run(async () => {
      await config.commit(current => ({ ...current, upstreams: { ...current.upstreams, [draft.namespace]: fromDraft(draft) } }))
      return created ? `${draft.namespace} added. Discovering its tools…` : `${draft.namespace} updated.`
    })
    if (saved) setEditor(null)
  }

  const toggle = (name: string, enabled: boolean) =>
    run(async () => {
      await config.commit(current => {
        const upstream = current.upstreams[name]
        if (!upstream) throw new Error(`${name} no longer exists in the configuration.`)
        return { ...current, upstreams: { ...current.upstreams, [name]: { ...upstream, enabled } } }
      })
      return `${name} ${enabled ? 'enabled' : 'disabled'}.`
    })

  const remove = async (name: string) => {
    const removed = await run(async () => {
      await config.commit(current => {
        const rest = { ...current.upstreams }
        delete rest[name]
        return { ...current, upstreams: rest }
      })
      return `${name} removed.`
    })
    if (removed) setPendingDelete(null)
  }

  const test = (name: string) =>
    run(async () => {
      const result = await gatewayApi.testUpstream(name)
      return result.available ? `${name} is reachable · ${plural(result.toolCount ?? 0, 'tool')}.` : `${name} did not respond.`
    })

  const refresh = (name: string) =>
    run(async () => {
      await gatewayApi.refreshUpstream(name)
      return `${name} tool catalog refreshed.`
    })

  const addButton = <Button variant="primary" icon={Plus} onClick={() => setEditor({ mode: 'create', draft: newDraft() })} disabled={!config.saved}>Add upstream</Button>

  return (
    <>
      <PageHeader
        title="Upstreams"
        description="MCP servers whose tools this gateway exposes through tools_search and tools_execute."
        actions={addButton}
      />

      <Card>
        <CardHeader title={plural(upstreams.length, 'upstream')} description="Status updates live as the gateway discovers tools." />
        {upstreams.length === 0 ? (
          <EmptyState
            icon={GitBranch}
            title="No upstreams configured"
            description="Add a local command (stdio) or a remote Streamable HTTP server to start discovering tools."
            actionLabel="Add upstream"
            onAction={() => setEditor({ mode: 'create', draft: newDraft() })}
          />
        ) : (
          <UpstreamTable
            upstreams={upstreams}
            busy={busy}
            onTest={test}
            onRefresh={refresh}
            onEdit={openEditor}
            onToggle={toggle}
            onDelete={setPendingDelete}
          />
        )}
      </Card>

      {editor && (
        <UpstreamEditorDialog
          mode={editor.mode}
          initial={editor.draft}
          takenNamespaces={namespaces}
          saving={busy}
          onSave={draft => void saveEditor(draft)}
          onClose={() => setEditor(null)}
        />
      )}

      {pendingDelete && (
        <ConfirmDialog
          title={`Remove ${pendingDelete}?`}
          confirmLabel="Remove upstream"
          tone="danger"
          busy={busy}
          onConfirm={() => void remove(pendingDelete)}
          onCancel={() => setPendingDelete(null)}
        >
          Clients will no longer find or call tools from <code>{pendingDelete}</code>. Policy rules that mention it are kept.
        </ConfirmDialog>
      )}
    </>
  )
}
