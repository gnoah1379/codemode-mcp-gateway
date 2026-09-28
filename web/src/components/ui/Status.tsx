import type { ReactNode } from 'react'

export type Tone = 'success' | 'danger' | 'warning' | 'info' | 'neutral'

export function Badge({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`badge badge-${tone}`}>{children}</span>
}

export function StatusDot({ tone = 'neutral', pulse = false }: { tone?: Tone; pulse?: boolean }) {
  return <span className={['dot', `dot-${tone}`, pulse && 'dot-pulse'].filter(Boolean).join(' ')} aria-hidden />
}

const EXECUTION_TONES: Record<string, Tone> = {
  succeeded: 'success',
  running: 'info',
  queued: 'info',
  failed: 'danger',
  timed_out: 'danger',
  cancelled: 'warning',
}

const EXECUTION_LABELS: Record<string, string> = {
  succeeded: 'Succeeded',
  running: 'Running',
  queued: 'Queued',
  failed: 'Failed',
  timed_out: 'Timed out',
  cancelled: 'Cancelled',
}

export function executionTone(status: string): Tone {
  return EXECUTION_TONES[status] ?? 'neutral'
}

export function ExecutionStatusBadge({ status }: { status: string }) {
  const tone = executionTone(status)
  return (
    <Badge tone={tone}>
      <StatusDot tone={tone} pulse={status === 'running'} />
      {EXECUTION_LABELS[status] ?? status}
    </Badge>
  )
}

export function UpstreamStatusBadge({ enabled, available }: { enabled: boolean; available: boolean }) {
  if (!enabled) return <Badge>Disabled</Badge>
  return available ? <Badge tone="success">Healthy</Badge> : <Badge tone="warning">Unavailable</Badge>
}

export function upstreamTone(enabled: boolean, available: boolean): Tone {
  if (!enabled) return 'neutral'
  return available ? 'success' : 'warning'
}
