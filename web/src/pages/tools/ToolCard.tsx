import { ChevronDown, Code2 } from 'lucide-react'
import type { Tool } from '../../api/types'
import { Badge, CodeBlock } from '../../components/ui'

type ToolCardProps = { tool: Tool; expanded: boolean; onToggle: () => void }

function accessBadge(tool: Tool) {
  if (!tool.available) return <Badge>Unavailable</Badge>
  return tool.allowed ? <Badge tone="success">Allowed</Badge> : <Badge tone="danger">Denied by policy</Badge>
}

export function ToolCard({ tool, expanded, onToggle }: ToolCardProps) {
  const fullName = `${tool.namespace}.${tool.name}`
  return (
    <article className="tool-card">
      <button type="button" className="tool-summary" aria-expanded={expanded} onClick={onToggle}>
        <span className="tool-glyph"><Code2 size={16} aria-hidden /></span>
        <span className="tool-title">
          <strong className="mono">{fullName}</strong>
          <span>{tool.description || 'No description provided by the upstream.'}</span>
        </span>
        {accessBadge(tool)}
        <ChevronDown className={expanded ? 'chevron open' : 'chevron'} size={16} aria-hidden />
      </button>
      {expanded && (
        <div className="tool-schemas">
          <CodeBlock title="Input schema" code={JSON.stringify(tool.inputSchema, null, 2)} />
          {tool.outputSchema != null && <CodeBlock title="Output schema" code={JSON.stringify(tool.outputSchema, null, 2)} />}
          {tool.matchedPattern && <p className="field-hint">Policy rule matched: <code>{tool.matchedPattern}</code></p>}
        </div>
      )}
    </article>
  )
}
