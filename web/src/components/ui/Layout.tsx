import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'
import { ArrowRight, CircleAlert, Info, TriangleAlert } from 'lucide-react'
import { Button } from './Button'

export function PageHeader({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page-header">
      <div>
        <h1>{title}</h1>
        {description && <p>{description}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </div>
  )
}

type EmptyStateProps = {
  icon: LucideIcon
  title: string
  description: ReactNode
  actionLabel?: string
  onAction?: () => void
}

export function EmptyState({ icon: Icon, title, description, actionLabel, onAction }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <span className="empty-icon"><Icon size={20} aria-hidden /></span>
      <strong>{title}</strong>
      <p>{description}</p>
      {actionLabel && onAction && <Button variant="ghost" size="sm" onClick={onAction}>{actionLabel}<ArrowRight size={14} aria-hidden /></Button>}
    </div>
  )
}

const CALLOUT_ICONS = { info: Info, warning: TriangleAlert, danger: CircleAlert }

export function Callout({ tone = 'info', title, children }: { tone?: keyof typeof CALLOUT_ICONS; title?: string; children: ReactNode }) {
  const Icon = CALLOUT_ICONS[tone]
  return (
    <div className={`callout callout-${tone}`} role={tone === 'info' ? 'note' : 'alert'}>
      <Icon size={16} aria-hidden />
      <div>
        {title && <strong>{title}</strong>}
        <div>{children}</div>
      </div>
    </div>
  )
}

/** Label/value pair used in stat strips and detail views. */
export function Stat({ label, value, caption }: { label: string; value: ReactNode; caption?: ReactNode }) {
  return (
    <div className="stat">
      <span className="stat-label">{label}</span>
      <strong className="stat-value">{value}</strong>
      {caption && <span className="stat-caption">{caption}</span>}
    </div>
  )
}
