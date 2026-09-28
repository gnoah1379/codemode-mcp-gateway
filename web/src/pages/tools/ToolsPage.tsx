import { useEffect, useState } from 'react'
import { Search } from 'lucide-react'
import { gatewayApi } from '../../api/gateway'
import { errorMessage } from '../../api/client'
import type { Tool } from '../../api/types'
import { Button, EmptyState, PageHeader } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { plural } from '../../lib/format'
import { ToolCard } from './ToolCard'

const PAGE_SIZE = 100
const SEARCH_DEBOUNCE_MS = 200

/** `dataEpoch` changes whenever gateway data refreshes, so results follow catalog updates. */
export function ToolsPage({ dataEpoch }: { dataEpoch: number }) {
  const notify = useToast()
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [tools, setTools] = useState<Tool[]>([])
  const [total, setTotal] = useState(0)
  const [expanded, setExpanded] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    const timer = window.setTimeout(() => {
      gatewayApi
        .tools(query.trim(), page, PAGE_SIZE)
        .then(result => {
          if (cancelled) return
          setTools(result.items)
          setTotal(result.total)
        })
        .catch(error => !cancelled && notify('error', errorMessage(error, 'Tool search failed.')))
    }, SEARCH_DEBOUNCE_MS)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [query, page, dataEpoch, notify])

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE))

  return (
    <>
      <PageHeader title="Tools" description="Every tool discovered from your upstreams, with the schema agents receive from tools_search." />

      <div className="search-box">
        <Search size={18} aria-hidden />
        <input
          type="search"
          value={query}
          placeholder="Search by name, namespace or description…"
          aria-label="Search tools"
          onChange={event => {
            setQuery(event.target.value)
            setPage(1)
          }}
        />
      </div>
      <p className="result-count">{plural(total, 'tool')}{query && ' match your search'}</p>

      {tools.length === 0 ? (
        <EmptyState icon={Search} title="No tools found" description="Try a different search, or add an upstream to discover tools." />
      ) : (
        <div className="tool-list">
          {tools.map(tool => {
            const id = `${tool.namespace}.${tool.name}`
            return <ToolCard key={id} tool={tool} expanded={expanded === id} onToggle={() => setExpanded(expanded === id ? null : id)} />
          })}
        </div>
      )}

      {pageCount > 1 && (
        <div className="pagination">
          <Button size="sm" disabled={page === 1} onClick={() => setPage(value => value - 1)}>Previous</Button>
          <span>Page {page} of {pageCount}</span>
          <Button size="sm" disabled={page >= pageCount} onClick={() => setPage(value => value + 1)}>Next</Button>
        </div>
      )}
    </>
  )
}
