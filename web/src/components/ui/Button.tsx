import type { ButtonHTMLAttributes, ReactNode } from 'react'
import { LoaderCircle, type LucideIcon } from 'lucide-react'

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
  icon?: LucideIcon
  loading?: boolean
  children?: ReactNode
}

export function Button({ variant = 'secondary', size = 'md', icon: Icon, loading = false, className, children, disabled, type = 'button', ...rest }: ButtonProps) {
  const classes = ['btn', `btn-${variant}`, `btn-${size}`, className].filter(Boolean).join(' ')
  return (
    <button type={type} className={classes} disabled={disabled || loading} {...rest}>
      {loading ? <LoaderCircle className="spin" size={16} aria-hidden /> : Icon && <Icon size={16} aria-hidden />}
      {children}
    </button>
  )
}

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { icon: LucideIcon; label: string }

/** Square button that shows only an icon; `label` is used for the tooltip and screen readers. */
export function IconButton({ icon: Icon, label, className, type = 'button', ...rest }: IconButtonProps) {
  return (
    <button type={type} className={['icon-btn', className].filter(Boolean).join(' ')} title={label} aria-label={label} {...rest}>
      <Icon size={16} aria-hidden />
    </button>
  )
}
