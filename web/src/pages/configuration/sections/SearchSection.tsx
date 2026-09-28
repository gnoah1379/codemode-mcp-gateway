import { ListFilter, Search } from 'lucide-react'
import { Card, CardBody, CardHeader, NumberField, SelectField } from '../../../components/ui'
import type { SearchConfig } from '../../../config/types'
import { formatBytes } from '../../../lib/format'
import type { SectionProps } from './types'

export function SearchSection({ value, onChange, errors }: SectionProps<SearchConfig>) {
  const set = (patch: Partial<SearchConfig>) => onChange({ ...value, ...patch })

  return (
    <>
      <Card>
        <CardHeader icon={Search} title="Search provider" description="How tools_search matches an agent's request against the tool catalog." />
        <CardBody>
          <SelectField
            label="Provider"
            value={value.provider}
            options={[{ value: 'keyword', label: 'Keyword — ranks by name, namespace and description' }]}
            onChange={provider => set({ provider })}
            hint="Keyword is the only provider available in this release."
          />
        </CardBody>
      </Card>

      <Card>
        <CardHeader icon={ListFilter} title="Result limits" description="Keep search results small so they do not flood the agent's context." />
        <CardBody>
          <div className="form-grid">
            <NumberField
              label="Default results"
              value={value.default_limit}
              onChange={default_limit => set({ default_limit })}
              min={1}
              unit="tools"
              error={errors['search.default_limit']}
              hint="Returned when the agent does not ask for a specific number."
            />
            <NumberField
              label="Maximum results"
              value={value.max_limit}
              onChange={max_limit => set({ max_limit })}
              min={1}
              max={1000}
              unit="tools"
              error={errors['search.max_limit']}
              hint="Upper bound an agent may request."
            />
            <NumberField
              label="Maximum response size"
              value={value.max_response_bytes}
              onChange={max_response_bytes => set({ max_response_bytes })}
              min={2}
              unit="bytes"
              error={errors['search.max_response_bytes']}
              hint={`≈ ${formatBytes(value.max_response_bytes || 0)}. Larger results are truncated.`}
            />
          </div>
        </CardBody>
      </Card>
    </>
  )
}
