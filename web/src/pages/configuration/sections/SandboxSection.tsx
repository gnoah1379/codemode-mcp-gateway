import { Cpu, Gauge, Timer } from 'lucide-react'
import { Card, CardBody, CardHeader, NumberField, SelectField } from '../../../components/ui'
import type { SandboxConfig } from '../../../config/types'
import { formatBytes, formatDuration } from '../../../lib/format'
import type { SectionProps } from './types'

export function SandboxSection({ value, onChange, errors }: SectionProps<SandboxConfig>) {
  const set = (patch: Partial<SandboxConfig>) => onChange({ ...value, ...patch })
  const setOptions = (patch: Partial<SandboxConfig['options']>) => set({ options: { ...value.options, ...patch } })

  return (
    <>
      <Card>
        <CardHeader icon={Timer} title="Time & concurrency" description="Each tools_execute call runs its script in a fresh, isolated worker." />
        <CardBody>
          <SelectField
            label="Sandbox runtime"
            value={value.provider}
            options={[{ value: 'deno', label: 'Deno (V8) worker process' }]}
            onChange={provider => set({ provider })}
          />
          <div className="form-grid">
            <NumberField
              label="Timeout"
              value={value.timeout_ms}
              onChange={timeout_ms => set({ timeout_ms })}
              min={1}
              unit="ms"
              error={errors['sandbox.timeout_ms']}
              hint={`≈ ${formatDuration(value.timeout_ms || 0)}. The worker is stopped when the deadline passes.`}
            />
            <NumberField
              label="Concurrent executions"
              value={value.max_concurrent_executions}
              onChange={max_concurrent_executions => set({ max_concurrent_executions })}
              min={1}
              unit="workers"
              error={errors['sandbox.max_concurrent_executions']}
            />
            <NumberField
              label="Queue size"
              value={value.max_queue_size}
              onChange={max_queue_size => set({ max_queue_size })}
              min={0}
              unit="requests"
              error={errors['sandbox.max_queue_size']}
              hint="Requests waiting for a free worker; more are rejected."
            />
          </div>
        </CardBody>
      </Card>

      <Card>
        <CardHeader icon={Gauge} title="Tool call quotas" description="Limits on what one script may do through tools.call()." />
        <CardBody>
          <div className="form-grid">
            <NumberField
              label="Tool calls per execution"
              value={value.max_tool_calls}
              onChange={max_tool_calls => set({ max_tool_calls })}
              min={1}
              unit="calls"
              error={errors['sandbox.max_tool_calls']}
            />
            <NumberField
              label="Parallel tool calls"
              value={value.max_parallel_tool_calls}
              onChange={max_parallel_tool_calls => set({ max_parallel_tool_calls })}
              min={1}
              unit="calls"
              error={errors['sandbox.max_parallel_tool_calls']}
              hint="How many calls from Promise.all may be in flight at once."
            />
          </div>
        </CardBody>
      </Card>

      <Card>
        <CardHeader icon={Cpu} title="Memory & output" description="Resource ceilings for a single worker." />
        <CardBody>
          <div className="form-grid">
            <NumberField
              label="JavaScript heap"
              value={value.options.heap_limit_mb}
              onChange={heap_limit_mb => setOptions({ heap_limit_mb })}
              min={16}
              unit="MiB"
              error={errors['sandbox.options.heap_limit_mb']}
            />
            <NumberField
              label="Worker process memory"
              value={value.options.worker_memory_limit_mb}
              onChange={worker_memory_limit_mb => setOptions({ worker_memory_limit_mb })}
              min={17}
              unit="MiB"
              error={errors['sandbox.options.worker_memory_limit_mb']}
              hint="Must be larger than the heap."
            />
            <NumberField
              label="Maximum result size"
              value={value.max_output_bytes}
              onChange={max_output_bytes => set({ max_output_bytes })}
              min={1}
              unit="bytes"
              error={errors['sandbox.max_output_bytes']}
              hint={`≈ ${formatBytes(value.max_output_bytes || 0)}, at most 8 MiB. The value a script returns must fit.`}
            />
          </div>
        </CardBody>
      </Card>
    </>
  )
}
