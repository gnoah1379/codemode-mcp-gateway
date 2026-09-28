import type { ValidationErrors } from '../../../config/validation'

/** Contract shared by every configuration section: edit one slice of the draft. */
export type SectionProps<T> = {
  value: T
  onChange: (value: T) => void
  /** Errors keyed by full config path, e.g. `sandbox.timeout_ms`. */
  errors: ValidationErrors
}
