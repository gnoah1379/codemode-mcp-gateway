import { useState, type ReactNode } from 'react'
import { Plus, X } from 'lucide-react'
import { Button, IconButton } from './Button'

type ListEditorProps = {
  label: string
  items: string[]
  onChange: (items: string[]) => void
  placeholder?: string
  addLabel?: string
  hint?: ReactNode
  /** Error message for each row, keyed by index. */
  errors?: Record<number, string | undefined>
  emptyText?: string
}

/** Ordered list of short strings (command arguments, policy patterns) with add, edit and remove. */
export function ListEditor({ label, items, onChange, placeholder, addLabel = 'Add', hint, errors = {}, emptyText }: ListEditorProps) {
  const [pending, setPending] = useState('')

  const add = () => {
    const value = pending.trim()
    if (!value) return
    onChange([...items, value])
    setPending('')
  }
  const update = (index: number, value: string) => onChange(items.map((item, current) => (current === index ? value : item)))
  const remove = (index: number) => onChange(items.filter((_, current) => current !== index))

  return (
    <div className="list-editor">
      <div className="field-label">{label}</div>
      {items.length === 0 && emptyText && <p className="list-empty">{emptyText}</p>}
      {items.map((item, index) => (
        <div className="list-row" key={index}>
          <div className="list-cell">
            <input
              className="input mono"
              value={item}
              aria-label={`${label} ${index + 1}`}
              aria-invalid={Boolean(errors[index])}
              onChange={event => update(index, event.target.value)}
            />
            {errors[index] && <p className="field-error">{errors[index]}</p>}
          </div>
          <IconButton icon={X} label="Remove" onClick={() => remove(index)} />
        </div>
      ))}
      <div className="list-row">
        <input
          className="input mono"
          value={pending}
          placeholder={placeholder}
          aria-label={`New ${label.toLowerCase()}`}
          onChange={event => setPending(event.target.value)}
          onKeyDown={event => {
            if (event.key === 'Enter') {
              event.preventDefault()
              add()
            }
          }}
        />
        <Button size="sm" icon={Plus} onClick={add} disabled={!pending.trim()}>{addLabel}</Button>
      </div>
      {hint && <p className="field-hint">{hint}</p>}
    </div>
  )
}
