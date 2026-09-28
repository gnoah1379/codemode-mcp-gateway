import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { IconButton } from './Button'

type CodeBlockProps = { title: string; code: string; maxHeight?: number }

export function CodeBlock({ title, code, maxHeight = 320 }: CodeBlockProps) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // Clipboard access can be denied; the text remains selectable.
    }
  }

  return (
    <div className="code-block">
      <div className="code-block-header">
        <span>{title}</span>
        <IconButton icon={copied ? Check : Copy} label={copied ? 'Copied' : 'Copy'} onClick={() => void copy()} />
      </div>
      <pre style={{ maxHeight }}>{code}</pre>
    </div>
  )
}
