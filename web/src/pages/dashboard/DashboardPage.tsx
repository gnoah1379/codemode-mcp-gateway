import { Activity, ArrowRight, GitBranch, Search, Settings2, Terminal } from 'lucide-react'
import type { Execution, Metrics, UpstreamStatus } from '../../api/types'
import { isActiveExecution } from '../../api/types'
import type { PageId } from '../../app/navigation'
import { Button, Card, CardHeader, EmptyState, ExecutionStatusBadge, PageHeader, Stat, StatusDot, UpstreamStatusBadge, upstreamTone } from '../../components/ui'
import { formatBytes, formatDuration, plural, timeAgo } from '../../lib/format'

type DashboardPageProps = {
  upstreams: UpstreamStatus[]
  executions: Execution[]
  metrics: Metrics
  onNavigate: (page: PageId) => void
  onOpenExecution: (id: string) => void
}

export function DashboardPage({ upstreams, executions, metrics, onNavigate, onOpenExecution }: DashboardPageProps) {
  const active = executions.filter(isActiveExecution).length
  const failed = metrics.executions.failed + metrics.executions.timedOut

  return (
    <>
      <PageHeader
        title="Overview"
        description="Connected MCP servers, tool catalog and recent Code Mode executions."
        actions={<Button icon={Settings2} onClick={() => onNavigate('configuration')}>Settings</Button>}
      />

      <div className="stat-grid">
        <Card><Stat label="Healthy upstreams" value={`${metrics.upstreams.healthy} / ${metrics.upstreams.total}`} caption="enabled servers responding" /></Card>
        <Card><Stat label="Available tools" value={metrics.tools.available} caption={`of ${metrics.tools.discovered} discovered`} /></Card>
        <Card><Stat label="Active executions" value={active} caption="running or queued" /></Card>
        <Card><Stat label="Tool calls" value={metrics.toolCalls} caption="within the retention window" /></Card>
      </div>

      <div className="grid-2">
        <Card>
          <CardHeader
            title="Upstream servers"
            description="Live discovery status"
            actions={<Button variant="ghost" size="sm" onClick={() => onNavigate('upstreams')}>Manage <ArrowRight size={14} aria-hidden /></Button>}
          />
          {upstreams.length === 0 ? (
            <EmptyState
              icon={GitBranch}
              title="No upstreams yet"
              description="Add an MCP server so clients can search and call its tools."
              actionLabel="Add upstream"
              onAction={() => onNavigate('upstreams')}
            />
          ) : (
            <ul className="row-list">
              {upstreams.slice(0, 6).map(upstream => (
                <li key={upstream.name} className="row-item">
                  <StatusDot tone={upstreamTone(upstream.enabled, upstream.available)} />
                  <div className="row-main">
                    <strong>{upstream.name}</strong>
                    <span>{upstream.description}</span>
                  </div>
                  <span className="row-meta">{plural(upstream.tool_count, 'tool')}</span>
                  <UpstreamStatusBadge enabled={upstream.enabled} available={upstream.available} />
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card>
          <CardHeader
            title="Recent executions"
            description="Latest tools_execute calls"
            actions={<Button variant="ghost" size="sm" onClick={() => onNavigate('executions')}>View all <ArrowRight size={14} aria-hidden /></Button>}
          />
          {executions.length === 0 ? (
            <EmptyState icon={Terminal} title="No executions yet" description="Executions appear here when a client calls tools_execute." />
          ) : (
            <ul className="row-list">
              {executions.slice(0, 6).map(execution => (
                <li key={execution.id}>
                  <button type="button" className="row-item row-button" onClick={() => onOpenExecution(execution.id)}>
                    <div className="row-main">
                      <strong className="mono">{execution.id.slice(0, 8)}</strong>
                      <span>{timeAgo(execution.startedAt)} · {plural(execution.calls.length, 'tool call')}</span>
                    </div>
                    <span className="row-meta">{formatDuration(execution.durationMs)}</span>
                    <ExecutionStatusBadge status={execution.status} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      <Card>
        <CardHeader title="Execution totals" description="Audit data stored in SQLite for the retention window" icon={Activity} />
        <div className="stat-strip">
          <Stat label="Total" value={metrics.executions.total} />
          <Stat label="Succeeded" value={metrics.executions.succeeded} />
          <Stat label="Failed or timed out" value={failed} />
          <Stat label="Cancelled" value={metrics.executions.cancelled} />
          <Stat label="Average duration" value={formatDuration(metrics.executions.averageDurationMs)} />
          <Stat label="Output returned" value={formatBytes(metrics.executions.outputBytes)} />
        </div>
      </Card>

      <div className="quick-links">
        <QuickLink icon={GitBranch} title="Connect a server" text="Add a stdio or HTTP MCP server" onClick={() => onNavigate('upstreams')} />
        <QuickLink icon={Search} title="Browse tools" text="Search names, descriptions and schemas" onClick={() => onNavigate('tools')} />
        <QuickLink icon={Settings2} title="Tune limits & policy" text="Timeouts, quotas and access rules" onClick={() => onNavigate('configuration')} />
      </div>
    </>
  )
}

function QuickLink({ icon: Icon, title, text, onClick }: { icon: typeof Search; title: string; text: string; onClick: () => void }) {
  return (
    <button type="button" className="quick-link" onClick={onClick}>
      <span className="quick-icon"><Icon size={18} aria-hidden /></span>
      <span className="quick-copy">
        <strong>{title}</strong>
        <span>{text}</span>
      </span>
      <ArrowRight size={16} aria-hidden />
    </button>
  )
}
