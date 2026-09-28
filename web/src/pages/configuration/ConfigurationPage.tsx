import { useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { gatewayApi } from '../../api/gateway'
import { errorMessage } from '../../api/client'
import { Button, Callout, EmptyState, PageHeader } from '../../components/ui'
import { errorsFor, hasErrors } from '../../config/validation'
import { configsEqual } from '../../config/yaml'
import type { GatewayConfigState } from '../../hooks/useGatewayConfig'
import { useToast } from '../../hooks/useToast'
import { SaveBar } from './SaveBar'
import { SECTIONS, type SectionId } from './sectionList'
import { ObservabilitySection } from './sections/ObservabilitySection'
import { PolicySection } from './sections/PolicySection'
import { PreviewSection } from './sections/PreviewSection'
import { SandboxSection } from './sections/SandboxSection'
import { SearchSection } from './sections/SearchSection'
import { ServerSection } from './sections/ServerSection'

type ConfigurationPageProps = {
  config: GatewayConfigState
  section: SectionId
  onSectionChange: (section: SectionId) => void
  dataEpoch: number
  onApplied: () => Promise<void>
}

export function ConfigurationPage({ config, section, onSectionChange, dataEpoch, onApplied }: ConfigurationPageProps) {
  const notify = useToast()
  const [validating, setValidating] = useState(false)
  const [reloading, setReloading] = useState(false)
  const { saved, draft, dirty, errors } = config

  if (!draft || !saved) {
    return (
      <>
        <PageHeader title="Settings" />
        <EmptyState icon={RefreshCw} title="Loading configuration…" description="Reading the active configuration from the gateway." />
      </>
    )
  }

  const sectionDirty = (id: SectionId) => id !== 'preview' && !configsEqual(saved[id], draft[id])
  const sectionInvalid = (id: SectionId) => id !== 'preview' && hasErrors(errorsFor(errors, id))
  const meta = SECTIONS.find(item => item.id === section) ?? SECTIONS[0]

  const save = async () => {
    try {
      await config.save()
      await onApplied().catch(() => undefined)
      notify('success', 'Settings saved and applied.')
    } catch (error) {
      notify('error', errorMessage(error, 'Could not save the settings.'))
    }
  }

  const validate = async () => {
    setValidating(true)
    try {
      const result = await config.validateRemotely()
      if (!result) return
      const degraded = result.degradedUpstreams.length
      notify(degraded ? 'info' : 'success', degraded ? `Settings are valid; ${degraded} upstream(s) are currently unreachable.` : 'Settings are valid.')
    } catch (error) {
      notify('error', errorMessage(error, 'Validation failed.'))
    } finally {
      setValidating(false)
    }
  }

  const reloadFromDisk = async () => {
    setReloading(true)
    try {
      await gatewayApi.reloadConfigFile()
      await config.load()
      await onApplied().catch(() => undefined)
      notify('success', 'Reloaded config.yaml from disk.')
    } catch (error) {
      notify('error', errorMessage(error, 'Could not reload config.yaml.'))
    } finally {
      setReloading(false)
    }
  }

  return (
    <>
      <PageHeader
        title="Settings"
        description="Every option in config.yaml, grouped by what it controls. Changes are validated before they are applied."
        actions={<Button icon={RefreshCw} loading={reloading} disabled={dirty} title={dirty ? 'Save or discard changes first' : undefined} onClick={() => void reloadFromDisk()}>Reload from file</Button>}
      />

      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              className={`settings-nav-item${section === id ? ' active' : ''}`}
              aria-current={section === id ? 'page' : undefined}
              onClick={() => onSectionChange(id)}
            >
              <Icon size={17} aria-hidden />
              <span>{label}</span>
              {sectionInvalid(id) ? <span className="nav-marker invalid" title="Has errors" /> : sectionDirty(id) && <span className="nav-marker dirty" title="Unsaved changes" />}
            </button>
          ))}
        </nav>

        <div className="settings-content">
          <div className="settings-heading">
            <h2>{meta.label}</h2>
            <p>{meta.description}</p>
          </div>

          {section === 'server' && <ServerSection value={draft.server} onChange={value => config.updateSection('server', value)} errors={errors} />}
          {section === 'search' && <SearchSection value={draft.search} onChange={value => config.updateSection('search', value)} errors={errors} />}
          {section === 'sandbox' && <SandboxSection value={draft.sandbox} onChange={value => config.updateSection('sandbox', value)} errors={errors} />}
          {section === 'policy' && <PolicySection value={draft.policy} onChange={value => config.updateSection('policy', value)} errors={errors} hasUnsavedChanges={sectionDirty('policy')} />}
          {section === 'observability' && <ObservabilitySection value={draft.observability} onChange={value => config.updateSection('observability', value)} errors={errors} />}
          {section === 'preview' && <PreviewSection draft={draft} dataEpoch={dataEpoch} />}

          {section !== 'preview' && (
            <Callout>Upstream servers are managed on the <a href="#/upstreams">Upstreams</a> page and saved immediately.</Callout>
          )}
        </div>
      </div>

      {dirty && (
        <SaveBar
          invalidSections={SECTIONS.filter(item => sectionInvalid(item.id)).map(item => item.label)}
          saving={config.saving}
          validating={validating}
          onDiscard={config.discard}
          onValidate={() => void validate()}
          onSave={() => void save()}
        />
      )}
    </>
  )
}
