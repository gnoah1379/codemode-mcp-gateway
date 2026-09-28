import { Pencil, Play, RefreshCw, Trash2 } from 'lucide-react'
import type { UpstreamStatus } from '../../api/types'
import { Badge, Button, IconButton, StatusDot, Switch, UpstreamStatusBadge, upstreamTone } from '../../components/ui'

type UpstreamTableProps = {
  upstreams: UpstreamStatus[]
  busy: boolean
  onTest: (name: string) => void
  onRefresh: (name: string) => void
  onEdit: (name: string) => void
  onToggle: (name: string, enabled: boolean) => void
  onDelete: (name: string) => void
}

export function UpstreamTable({ upstreams, busy, onTest, onRefresh, onEdit, onToggle, onDelete }: UpstreamTableProps) {
  return (
    <div className="table-scroll">
      <table className="table">
        <thead>
          <tr>
            <th>Upstream</th>
            <th>Transport</th>
            <th>Status</th>
            <th className="numeric">Tools</th>
            <th>Enabled</th>
            <th aria-label="Actions" />
          </tr>
        </thead>
        <tbody>
          {upstreams.map(upstream => (
            <tr key={upstream.name}>
              <td>
                <div className="cell-title">
                  <StatusDot tone={upstreamTone(upstream.enabled, upstream.available)} />
                  <div>
                    <strong className="mono">{upstream.name}</strong>
                    <span className="cell-subtitle">{upstream.description}</span>
                    {upstream.enabled && !upstream.available && upstream.last_error && (
                      <span className="cell-error">{upstream.last_error}</span>
                    )}
                  </div>
                </div>
              </td>
              <td><Badge>{upstream.transport === 'streamable_http' ? 'HTTP' : 'stdio'}</Badge></td>
              <td><UpstreamStatusBadge enabled={upstream.enabled} available={upstream.available} /></td>
              <td className="numeric mono">{upstream.tool_count}</td>
              <td>
                <Switch
                  label={`Enable ${upstream.name}`}
                  hideLabel
                  checked={upstream.enabled}
                  disabled={busy}
                  onChange={enabled => onToggle(upstream.name, enabled)}
                />
              </td>
              <td>
                <div className="row-actions">
                  <Button size="sm" icon={Play} disabled={busy} onClick={() => onTest(upstream.name)}>Test</Button>
                  <IconButton icon={RefreshCw} label="Refresh tool catalog" disabled={busy} onClick={() => onRefresh(upstream.name)} />
                  <IconButton icon={Pencil} label="Edit" disabled={busy} onClick={() => onEdit(upstream.name)} />
                  <IconButton icon={Trash2} label="Delete" className="danger" disabled={busy} onClick={() => onDelete(upstream.name)} />
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
