import { useCallback, useState, type ReactNode } from 'react'
import { CircleCheck, CircleX, Info, X } from 'lucide-react'
import { ToastContext, type ToastTone } from '../../hooks/useToast'

type Toast = { id: number; tone: ToastTone; message: string }

const ICONS = { success: CircleCheck, error: CircleX, info: Info }
const DURATION_MS: Record<ToastTone, number> = { success: 3500, info: 3500, error: 7000 }
const MAX_VISIBLE = 3

let nextId = 0

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([])

  const dismiss = useCallback((id: number) => setToasts(list => list.filter(toast => toast.id !== id)), [])

  const notify = useCallback((tone: ToastTone, message: string) => {
    const id = ++nextId
    setToasts(list => [...list.slice(-(MAX_VISIBLE - 1)), { id, tone, message }])
    window.setTimeout(() => dismiss(id), DURATION_MS[tone])
  }, [dismiss])

  return (
    <ToastContext.Provider value={notify}>
      {children}
      <div className="toast-stack" role="status" aria-live="polite">
        {toasts.map(({ id, tone, message }) => {
          const Icon = ICONS[tone]
          return (
            <div key={id} className={`toast toast-${tone}`}>
              <Icon size={17} aria-hidden />
              <span>{message}</span>
              <button type="button" aria-label="Dismiss" onClick={() => dismiss(id)}><X size={14} /></button>
            </div>
          )
        })}
      </div>
    </ToastContext.Provider>
  )
}
