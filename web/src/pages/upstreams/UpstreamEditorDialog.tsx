import { useMemo, useState } from 'react'
import { Globe, Plus, Save, Terminal } from 'lucide-react'
import { Button, Callout, Field, KeyValueEditor, ListEditor, Modal, SegmentedControl, Switch, TextAreaField, TextField, type Segment } from '../../components/ui'
import type { UpstreamDraft } from '../../config/upstreamDraft'
import { hasErrors, validateUpstreamDraft, type ValidationErrors } from '../../config/validation'

type UpstreamEditorDialogProps = {
  mode: 'create' | 'edit'
  initial: UpstreamDraft
  /** Namespaces that a new upstream may not reuse. */
  takenNamespaces: string[]
  saving: boolean
  onSave: (draft: UpstreamDraft) => void
  onClose: () => void
}

const TRANSPORTS: Segment<UpstreamDraft['transport']>[] = [
  { value: 'stdio', label: 'Local command (stdio)', icon: Terminal },
  { value: 'streamable_http', label: 'Remote URL (HTTP)', icon: Globe },
]

/** Keeps only the errors under `prefix.` and strips the prefix, e.g. `env.0.key` → `0.key`. */
function scoped(errors: ValidationErrors, prefix: string): Record<string, string> {
  return Object.fromEntries(
    Object.entries(errors)
      .filter(([path]) => path.startsWith(`${prefix}.`))
      .map(([path, message]) => [path.slice(prefix.length + 1), message]),
  )
}

export function UpstreamEditorDialog({ mode, initial, takenNamespaces, saving, onSave, onClose }: UpstreamEditorDialogProps) {
  const [draft, setDraft] = useState(initial)
  const [submitted, setSubmitted] = useState(false)

  const errors = useMemo(
    () => validateUpstreamDraft(draft, mode === 'create' ? takenNamespaces : undefined),
    [draft, mode, takenNamespaces],
  )
  const visibleErrors = submitted ? errors : {}
  const update = <K extends keyof UpstreamDraft>(key: K, value: UpstreamDraft[K]) => setDraft(current => ({ ...current, [key]: value }))

  const submit = () => {
    setSubmitted(true)
    if (!hasErrors(errors)) onSave(draft)
  }

  return (
    <Modal
      title={mode === 'create' ? 'Add upstream' : `Edit ${initial.namespace}`}
      eyebrow={mode === 'create' ? 'New MCP server' : 'Upstream settings'}
      size="lg"
      onClose={onClose}
      onSubmit={submit}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" icon={mode === 'create' ? Plus : Save} loading={saving}>
            {mode === 'create' ? 'Add upstream' : 'Save changes'}
          </Button>
        </>
      }
    >
      <div className="form-section">
        <h3>General</h3>
        <div className="form-grid">
          <TextField
            label="Namespace"
            value={draft.namespace}
            onChange={value => update('namespace', value.toLowerCase())}
            placeholder="github"
            disabled={mode === 'edit'}
            monospace
            autoFocus={mode === 'create'}
            error={visibleErrors.namespace}
            hint={mode === 'edit' ? 'The namespace cannot change after creation.' : <>Prefix for tool names, e.g. <code>{draft.namespace || 'github'}.list_issues</code>.</>}
          />
          <div className="field">
            <span className="field-label">Status</span>
            <Switch
              checked={draft.enabled}
              onChange={value => update('enabled', value)}
              label={draft.enabled ? 'Enabled' : 'Disabled'}
              description="Disabled servers stay configured but their tools are hidden."
            />
          </div>
        </div>
        <TextAreaField
          label="Description"
          value={draft.description}
          onChange={value => update('description', value)}
          placeholder="Repository, issue and pull request operations."
          rows={2}
          maxLength={512}
          error={visibleErrors.description}
          hint="Shown to agents in the tools_search description, so say what this server is for."
        />
      </div>

      <div className="form-section">
        <h3>Connection</h3>
        <Field label="Transport">
          {() => <SegmentedControl label="Transport" value={draft.transport} options={TRANSPORTS} onChange={value => update('transport', value)} />}
        </Field>

        {draft.transport === 'stdio' ? (
          <>
            <TextField
              label="Command"
              value={draft.command}
              onChange={value => update('command', value)}
              placeholder="npx"
              monospace
              error={visibleErrors.command}
              hint="Executable launched directly by the gateway. Shell syntax such as pipes or && is not interpreted."
            />
            <ListEditor
              label="Arguments"
              items={draft.args}
              onChange={value => update('args', value)}
              placeholder="-y"
              addLabel="Add argument"
              emptyText="No arguments."
              hint="Each row is passed as one argument, in order."
            />
            <KeyValueEditor
              label="Environment variables"
              rows={draft.env}
              onChange={value => update('env', value)}
              keyLabel="Name"
              valueLabel="Value"
              keyPlaceholder="GITHUB_TOKEN"
              valuePlaceholder="ghp_xxxxxxxxxxxx"
              addLabel="Add variable"
              emptyText="The server starts with no extra environment variables."
              errors={scoped(visibleErrors, 'env')}
            />
          </>
        ) : (
          <>
            <TextField
              label="Server URL"
              type="url"
              value={draft.url}
              onChange={value => update('url', value)}
              placeholder="https://mcp.example.com/mcp"
              monospace
              error={visibleErrors.url}
              hint="Streamable HTTP endpoint. Plain http is allowed only for localhost."
            />
            <KeyValueEditor
              label="Request headers"
              rows={draft.headers}
              onChange={value => update('headers', value)}
              keyLabel="Header"
              valueLabel="Value"
              keyPlaceholder="Authorization"
              valuePlaceholder="Bearer sk-xxxxxxxx"
              addLabel="Add header"
              emptyText="No extra headers are sent."
              errors={scoped(visibleErrors, 'headers')}
            />
          </>
        )}

        <Callout title="Values are used as entered">
          Values are saved in <code>config.yaml</code>, which only your user can read, and apply right after saving.
          Local servers also get <code>PATH</code>, <code>HOME</code> and a few other basic variables so launchers like <code>npx</code> or <code>docker</code> work.
          An <code>Authorization</code> header value must include its scheme, e.g. <code>Bearer …</code>.
        </Callout>
      </div>
    </Modal>
  )
}
