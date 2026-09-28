import { createContext, useContext } from 'react'

export type ToastTone = 'success' | 'error' | 'info'

export type Notify = (tone: ToastTone, message: string) => void

export const ToastContext = createContext<Notify>(() => undefined)

/** Returns `notify(tone, message)`; requires a `<ToastProvider>` ancestor. */
export function useToast(): Notify {
  return useContext(ToastContext)
}
