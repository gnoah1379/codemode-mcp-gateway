import { CircleAlert, Save, Undo2 } from 'lucide-react'
import { Button } from '../../components/ui'

type SaveBarProps = {
  invalidSections: string[]
  saving: boolean
  validating: boolean
  onDiscard: () => void
  onValidate: () => void
  onSave: () => void
}

/** Sticky bar shown while the settings draft differs from the saved configuration. */
export function SaveBar({ invalidSections, saving, validating, onDiscard, onValidate, onSave }: SaveBarProps) {
  const invalid = invalidSections.length > 0
  return (
    <div className="save-bar" role="region" aria-label="Unsaved changes">
      <div className={`save-bar-message${invalid ? ' invalid' : ''}`}>
        {invalid ? (
          <>
            <CircleAlert size={16} aria-hidden />
            Fix the highlighted fields in {invalidSections.join(', ')} before saving.
          </>
        ) : (
          'You have unsaved changes.'
        )}
      </div>
      <div className="save-bar-actions">
        <Button variant="ghost" icon={Undo2} onClick={onDiscard} disabled={saving}>Discard</Button>
        <Button onClick={onValidate} loading={validating} disabled={invalid || saving}>Validate</Button>
        <Button variant="primary" icon={Save} onClick={onSave} loading={saving} disabled={invalid}>Save changes</Button>
      </div>
    </div>
  )
}
