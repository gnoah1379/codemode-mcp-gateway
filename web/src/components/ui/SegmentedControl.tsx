import type { LucideIcon } from 'lucide-react'

export type Segment<T extends string> = { value: T; label: string; icon?: LucideIcon }

type SegmentedControlProps<T extends string> = {
  value: T
  options: Segment<T>[]
  onChange: (value: T) => void
  label: string
  size?: 'sm' | 'md'
  iconOnly?: boolean
}

/** Mutually exclusive choice between a few options, e.g. transport type or default policy. */
export function SegmentedControl<T extends string>({ value, options, onChange, label, size = 'md', iconOnly = false }: SegmentedControlProps<T>) {
  return (
    <div className={`segmented segmented-${size}`} role="radiogroup" aria-label={label}>
      {options.map(({ value: optionValue, label: optionLabel, icon: Icon }) => (
        <button
          key={optionValue}
          type="button"
          role="radio"
          aria-checked={value === optionValue}
          className={value === optionValue ? 'active' : undefined}
          title={iconOnly ? optionLabel : undefined}
          aria-label={iconOnly ? optionLabel : undefined}
          onClick={() => onChange(optionValue)}
        >
          {Icon && <Icon size={15} aria-hidden />}
          {!iconOnly && optionLabel}
        </button>
      ))}
    </div>
  )
}
