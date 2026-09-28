import { useId, type ReactNode } from 'react'

type SwitchProps = {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  description?: ReactNode
  error?: string
  disabled?: boolean
  /** Keeps the label for screen readers only, e.g. inside a table row. */
  hideLabel?: boolean
}

/** On/off setting rendered as a labelled row: text on the left, toggle on the right. */
export function Switch({ checked, onChange, label, description, error, disabled, hideLabel = false }: SwitchProps) {
  const id = useId()
  return (
    <div className={['switch-row', hideLabel && 'switch-compact', error && 'field-invalid'].filter(Boolean).join(' ')}>
      <div className={hideLabel ? 'sr-only' : 'switch-copy'}>
        <label htmlFor={id}>{label}</label>
        {description && <p>{description}</p>}
        {error && <p className="field-error">{error}</p>}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        className="switch"
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <span className="switch-thumb" />
      </button>
    </div>
  )
}
