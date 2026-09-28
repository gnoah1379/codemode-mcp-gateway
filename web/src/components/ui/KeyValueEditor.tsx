import type { ReactNode } from 'react'
import { ArrowRight, Plus, X } from 'lucide-react'
import { Button, IconButton } from './Button'
import type { KeyValueRow } from '../../config/upstreamDraft'

type KeyValueEditorProps = {
  label: string
  rows: KeyValueRow[]
  onChange: (rows: KeyValueRow[]) => void
  keyLabel: string
  valueLabel: string
  keyPlaceholder?: string
  valuePlaceholder?: string
  addLabel?: string
  hint?: ReactNode
  emptyText?: string
  /** Errors keyed as `${index}.key` or `${index}.value`. */
  errors?: Record<string, string | undefined>
}

/** Editable table of name → value pairs, used for environment variables and HTTP headers. */
export function KeyValueEditor({ label, rows, onChange, keyLabel, valueLabel, keyPlaceholder, valuePlaceholder, addLabel = 'Add row', hint, emptyText, errors = {} }: KeyValueEditorProps) {
  const update = (index: number, patch: Partial<KeyValueRow>) =>
    onChange(rows.map((row, current) => (current === index ? { ...row, ...patch } : row)))
  const remove = (index: number) => onChange(rows.filter((_, current) => current !== index))

  return (
    <div className="kv-editor">
      <div className="field-label">{label}</div>
      {rows.length === 0 ? (
        emptyText && <p className="list-empty">{emptyText}</p>
      ) : (
        <div className="kv-header" aria-hidden>
          <span>{keyLabel}</span>
          <span />
          <span>{valueLabel}</span>
          <span />
        </div>
      )}
      {rows.map((row, index) => (
        <div className="kv-row" key={index}>
          <div className="list-cell">
            <input
              className="input mono"
              value={row.key}
              placeholder={keyPlaceholder}
              aria-label={`${keyLabel} ${index + 1}`}
              aria-invalid={Boolean(errors[`${index}.key`])}
              onChange={event => update(index, { key: event.target.value })}
            />
            {errors[`${index}.key`] && <p className="field-error">{errors[`${index}.key`]}</p>}
          </div>
          <ArrowRight className="kv-arrow" size={15} aria-hidden />
          <div className="list-cell">
            <input
              className="input mono"
              value={row.value}
              placeholder={valuePlaceholder}
              aria-label={`${valueLabel} ${index + 1}`}
              aria-invalid={Boolean(errors[`${index}.value`])}
              onChange={event => update(index, { value: event.target.value })}
            />
            {errors[`${index}.value`] && <p className="field-error">{errors[`${index}.value`]}</p>}
          </div>
          <IconButton icon={X} label="Remove row" onClick={() => remove(index)} />
        </div>
      ))}
      <div>
        <Button size="sm" variant="ghost" icon={Plus} onClick={() => onChange([...rows, { key: '', value: '' }])}>{addLabel}</Button>
      </div>
      {hint && <p className="field-hint">{hint}</p>}
    </div>
  )
}
