import { Activity, X } from 'lucide-react'
import { isActiveExecution, type Execution } from '../../api/types'
import { Badge, Button, Card, CardHeader, EmptyState, ExecutionStatusBadge, PageHeader } from '../../components/ui'
import { formatBytes, formatDuration, plural, timeAgo } from '../../lib/format'

type ExecutionsPageProps = {
  executions: Execution[]
  retentionDays?: number
  onOpen: (id: string) => void
  onCancel: (id: string) => void
}

export function ExecutionsPage({ executions, retentionDays, onOpen, onCancel }: ExecutionsPageProps) {
  return (
    <>
      <PageHeader
        title="Executions"
        description="Every tools_execute run with its tool calls. Only metadata is kept; code, arguments and results are never stored."
      />
      <Card>
        <CardHeader
          title={plural(executions.length, 'execution')}
          description="Most recent first · updates live"
          actions={retentionDays != null && <Badge>{retentionDays}-day retention</Badge>}
        />
        {executions.length === 0 ? (
          <EmptyState icon={Activity} title="No executions recorded" description="A run appears here as soon as a client calls tools_execute." />
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>Execution</th>
                  <th>Status</th>
                  <th className="numeric">Tool calls</th>
                  <th className="numeric">Duration</th>
                  <th className="numeric">Output</th>
                  <th>Started</th>
                  <th aria-label="Actions" />
                </tr>
              </thead>
              <tbody>
                {executions.map(execution => (
                  <tr key={execution.id}>
                    <td><button type="button" className="link mono" onClick={() => onOpen(execution.id)}>{execution.id.slice(0, 8)}</button></td>
                    <td><ExecutionStatusBadge status={execution.status} /></td>
                    <td className="numeric mono">{execution.calls.length}</td>
                    <td className="numeric mono">{formatDuration(execution.durationMs)}</td>
                    <td className="numeric mono">{formatBytes(execution.outputBytes)}</td>
                    <td>{timeAgo(execution.startedAt)}</td>
                    <td>
                      {isActiveExecution(execution) && (
                        <Button size="sm" variant="danger" icon={X} onClick={() => onCancel(execution.id)}>Cancel</Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </>
  )
}
