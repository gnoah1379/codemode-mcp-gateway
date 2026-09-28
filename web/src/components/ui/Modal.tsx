import { useEffect, useId, type FormEvent, type ReactNode } from 'react'
import { X } from 'lucide-react'
import { IconButton } from './Button'

type ModalProps = {
  title: string
  eyebrow?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  size?: 'sm' | 'md' | 'lg'
  /** When set, the dialog body is a form and Enter submits it. */
  onSubmit?: () => void
}

export function Modal({ title, eyebrow, onClose, children, footer, size = 'md', onSubmit }: ModalProps) {
  const titleId = useId()

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => event.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  const content = (
    <>
      <header className="modal-header">
        <div>
          {eyebrow && <div className="eyebrow">{eyebrow}</div>}
          <h2 id={titleId}>{title}</h2>
        </div>
        <IconButton icon={X} label="Close" onClick={onClose} />
      </header>
      <div className="modal-body">{children}</div>
      {footer && <footer className="modal-footer">{footer}</footer>}
    </>
  )

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    onSubmit?.()
  }

  return (
    <div className="modal-backdrop" onMouseDown={event => event.target === event.currentTarget && onClose()}>
      {onSubmit ? (
        <form className={`modal modal-${size}`} role="dialog" aria-modal="true" aria-labelledby={titleId} onSubmit={handleSubmit} noValidate>
          {content}
        </form>
      ) : (
        <div className={`modal modal-${size}`} role="dialog" aria-modal="true" aria-labelledby={titleId}>
          {content}
        </div>
      )}
    </div>
  )
}
