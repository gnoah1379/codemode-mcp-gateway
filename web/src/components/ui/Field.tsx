import { useId, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TextareaHTMLAttributes } from 'react'

type FieldProps = {
  label: string
  hint?: ReactNode
  error?: string
  /** Renders `children(id)` so the control can be linked to its label and messages. */
  children: (ids: { id: string; describedBy?: string; invalid: boolean }) => ReactNode
  className?: string
}

/** Label, control, hint and error message laid out consistently for every form input. */
export function Field({ label, hint, error, children, className }: FieldProps) {
  const id = useId()
  const messageId = `${id}-message`
  const hasMessage = Boolean(error || hint)
  return (
    <div className={['field', error && 'field-invalid', className].filter(Boolean).join(' ')}>
      <label className="field-label" htmlFor={id}>{label}</label>
      {children({ id, describedBy: hasMessage ? messageId : undefined, invalid: Boolean(error) })}
      {error ? (
        <p id={messageId} className="field-error">{error}</p>
      ) : (
        hint && <p id={messageId} className="field-hint">{hint}</p>
      )}
    </div>
  )
}

type TextFieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value'> & {
  label: string
  value: string
  onChange: (value: string) => void
  hint?: ReactNode
  error?: string
  monospace?: boolean
}

export function TextField({ label, value, onChange, hint, error, monospace, className, ...rest }: TextFieldProps) {
  return (
    <Field label={label} hint={hint} error={error} className={className}>
      {({ id, describedBy, invalid }) => (
        <input
          id={id}
          className={['input', monospace && 'mono'].filter(Boolean).join(' ')}
          value={value}
          aria-invalid={invalid}
          aria-describedby={describedBy}
          onChange={event => onChange(event.target.value)}
          {...rest}
        />
      )}
    </Field>
  )
}

type TextAreaFieldProps = Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, 'onChange' | 'value'> & {
  label: string
  value: string
  onChange: (value: string) => void
  hint?: ReactNode
  error?: string
}

export function TextAreaField({ label, value, onChange, hint, error, className, ...rest }: TextAreaFieldProps) {
  return (
    <Field label={label} hint={hint} error={error} className={className}>
      {({ id, describedBy, invalid }) => (
        <textarea
          id={id}
          className="input textarea"
          value={value}
          aria-invalid={invalid}
          aria-describedby={describedBy}
          onChange={event => onChange(event.target.value)}
          {...rest}
        />
      )}
    </Field>
  )
}

type NumberFieldProps = {
  label: string
  value: number
  onChange: (value: number) => void
  unit?: string
  min?: number
  max?: number
  step?: number
  hint?: ReactNode
  error?: string
}

/** Numeric input with a unit suffix. Empty input is reported as NaN so validation can flag it. */
export function NumberField({ label, value, onChange, unit, min, max, step = 1, hint, error }: NumberFieldProps) {
  return (
    <Field label={label} hint={hint} error={error}>
      {({ id, describedBy, invalid }) => (
        <div className="input-group">
          <input
            id={id}
            className="input mono"
            type="number"
            inputMode="numeric"
            min={min}
            max={max}
            step={step}
            value={Number.isNaN(value) ? '' : value}
            aria-invalid={invalid}
            aria-describedby={describedBy}
            onChange={event => onChange(event.target.value === '' ? Number.NaN : Number(event.target.value))}
          />
          {unit && <span className="input-suffix">{unit}</span>}
        </div>
      )}
    </Field>
  )
}

type SelectOption<T extends string> = { value: T; label: string }

type SelectFieldProps<T extends string> = Omit<SelectHTMLAttributes<HTMLSelectElement>, 'onChange' | 'value'> & {
  label: string
  value: T
  options: SelectOption<T>[]
  onChange: (value: T) => void
  hint?: ReactNode
  error?: string
}

export function SelectField<T extends string>({ label, value, options, onChange, hint, error, ...rest }: SelectFieldProps<T>) {
  return (
    <Field label={label} hint={hint} error={error}>
      {({ id, describedBy, invalid }) => (
        <select
          id={id}
          className="input select"
          value={value}
          aria-invalid={invalid}
          aria-describedby={describedBy}
          onChange={event => onChange(event.target.value as T)}
          {...rest}
        >
          {options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      )}
    </Field>
  )
}
