import { useState } from 'react'
import { Ban, Check, FlaskConical, ShieldCheck } from 'lucide-react'
import { gatewayApi } from '../../../api/gateway'
import { errorMessage } from '../../../api/client'
import type { PolicyDecision } from '../../../api/types'
import { Button, Callout, Card, CardBody, CardHeader, Field, ListEditor, SegmentedControl, TextField } from '../../../components/ui'
import type { PolicyConfig } from '../../../config/types'
import type { ValidationErrors } from '../../../config/validation'
import type { SectionProps } from './types'

function indexedErrors(errors: ValidationErrors, prefix: string): Record<number, string> {
  return Object.fromEntries(
    Object.entries(errors)
      .filter(([path]) => path.startsWith(`${prefix}.`))
      .map(([path, message]) => [Number(path.slice(prefix.length + 1)), message]),
  )
}

type PolicySectionProps = SectionProps<PolicyConfig> & { hasUnsavedChanges: boolean }

export function PolicySection({ value, onChange, errors, hasUnsavedChanges }: PolicySectionProps) {
  const set = (patch: Partial<PolicyConfig>) => onChange({ ...value, ...patch })

  return (
    <>
      <Card>
        <CardHeader icon={ShieldCheck} title="Default decision" description="Applies to tools that match no rule below." />
        <CardBody>
          <Field label="When no rule matches" hint={value.default === 'allow' ? 'Every tool is usable unless a deny rule matches.' : 'Only tools matching an allow rule are usable.'}>
            {() => (
              <SegmentedControl
                label="Default decision"
                value={value.default}
                onChange={decision => set({ default: decision })}
                options={[{ value: 'allow', label: 'Allow', icon: Check }, { value: 'deny', label: 'Deny', icon: Ban }]}
              />
            )}
          </Field>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="Rules" description="Patterns use namespace.tool with * as a wildcard, e.g. github.* or *.delete_*." />
        <CardBody>
          <Callout>Deny rules always win. Then allow rules, then the default decision. Denied tools are hidden from search and rejected on call.</Callout>
          <div className="form-grid">
            <ListEditor
              label="Allow"
              items={value.allow}
              onChange={allow => set({ allow })}
              placeholder="github.list_*"
              addLabel="Add rule"
              emptyText="No allow rules."
              errors={indexedErrors(errors, 'policy.allow')}
            />
            <ListEditor
              label="Deny"
              items={value.deny}
              onChange={deny => set({ deny })}
              placeholder="*.delete_*"
              addLabel="Add rule"
              emptyText="No deny rules."
              errors={indexedErrors(errors, 'policy.deny')}
            />
          </div>
        </CardBody>
      </Card>

      <PolicyTester hasUnsavedChanges={hasUnsavedChanges} />
    </>
  )
}

function PolicyTester({ hasUnsavedChanges }: { hasUnsavedChanges: boolean }) {
  const [name, setName] = useState('')
  const [decision, setDecision] = useState<PolicyDecision | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const evaluate = async () => {
    setBusy(true)
    setError('')
    try {
      setDecision(await gatewayApi.evaluatePolicy(name.trim()))
    } catch (failure) {
      setDecision(null)
      setError(errorMessage(failure, 'Could not evaluate the policy.'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Card>
      <CardHeader icon={FlaskConical} title="Test a tool name" description="Check how the active policy treats one tool." />
      <CardBody>
        {hasUnsavedChanges && <Callout tone="warning">The test uses the saved policy. Save your changes to test them.</Callout>}
        <div className="inline-form">
          <TextField
            label="Full tool name"
            value={name}
            onChange={next => {
              setName(next)
              setDecision(null)
            }}
            placeholder="github.delete_issue"
            monospace
            error={error || undefined}
            onKeyDown={event => event.key === 'Enter' && name.trim() && void evaluate()}
          />
          <Button onClick={() => void evaluate()} loading={busy} disabled={!name.trim()}>Evaluate</Button>
        </div>
        {decision && (
          <div className={`decision decision-${decision.allowed ? 'allow' : 'deny'}`}>
            {decision.allowed ? <Check size={16} aria-hidden /> : <Ban size={16} aria-hidden />}
            <strong>{decision.allowed ? 'Allowed' : 'Denied'}</strong>
            <span>{decision.matchedPattern ? <>by rule <code>{decision.matchedPattern}</code></> : 'by the default decision'}</span>
          </div>
        )}
      </CardBody>
    </Card>
  )
}
