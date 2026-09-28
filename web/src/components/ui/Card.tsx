import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

type CardProps = { children: ReactNode; className?: string }

export function Card({ children, className }: CardProps) {
  return <section className={['card', className].filter(Boolean).join(' ')}>{children}</section>
}

type CardHeaderProps = {
  title: string
  description?: ReactNode
  icon?: LucideIcon
  actions?: ReactNode
}

export function CardHeader({ title, description, icon: Icon, actions }: CardHeaderProps) {
  return (
    <header className="card-header">
      <div className="card-heading">
        {Icon && <span className="card-icon"><Icon size={16} aria-hidden /></span>}
        <div>
          <h2>{title}</h2>
          {description && <p>{description}</p>}
        </div>
      </div>
      {actions && <div className="card-actions">{actions}</div>}
    </header>
  )
}

export function CardBody({ children, className }: CardProps) {
  return <div className={['card-body', className].filter(Boolean).join(' ')}>{children}</div>
}

export function CardFooter({ children }: { children: ReactNode }) {
  return <footer className="card-footer">{children}</footer>
}
