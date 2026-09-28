import { HardDrive, ScrollText } from 'lucide-react'
import { Card, CardBody, CardHeader, NumberField, SelectField, TextField } from '../../../components/ui'
import { LOG_LEVELS, type ObservabilityConfig } from '../../../config/types'
import type { SectionProps } from './types'

const LEVEL_OPTIONS = LOG_LEVELS.map(level => ({ value: level, label: level }))

export function ObservabilitySection({ value, onChange, errors }: SectionProps<ObservabilityConfig>) {
  const set = (patch: Partial<ObservabilityConfig>) => onChange({ ...value, ...patch })

  return (
    <>
      <Card>
        <CardHeader icon={HardDrive} title="Audit storage" description="Execution history and tool call metadata are kept in SQLite." />
        <CardBody>
          <TextField
            label="Database file"
            value={value.database}
            onChange={() => undefined}
            readOnly
            monospace
            hint="Fixed while the gateway runs. To move it, edit config.yaml and restart the service."
          />
          <NumberField
            label="Retention"
            value={value.retention_days}
            onChange={retention_days => set({ retention_days })}
            min={0}
            unit="days"
            error={errors['observability.retention_days']}
            hint="Older execution records are deleted automatically."
          />
        </CardBody>
      </Card>

      <Card>
        <CardHeader icon={ScrollText} title="Logging" description="Structured JSON logs written by the gateway process." />
        <CardBody>
          <SelectField
            label="Log level"
            value={value.log_level}
            options={LEVEL_OPTIONS}
            onChange={log_level => set({ log_level })}
            hint="Applies from the next start; RUST_LOG overrides it when set."
          />
        </CardBody>
      </Card>
    </>
  )
}
