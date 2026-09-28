import { isActiveExecution, type Execution } from '../../api/types'
import { Badge, Button, ExecutionStatusBadge, Modal, Stat } from '../../components/ui'
import { formatBytes, formatDuration } from '../../lib/format'

type ExecutionDialogProps = {
  execution: Execution
  onClose: () => void
  onCancel: (id: string) => void
}

export function ExecutionDialog({ execution, onClose, onCancel }: ExecutionDialogProps) {
  return (
    <Modal
      title={execution.id}
      eyebrow="Execution"
      size="lg"
      onClose={onClose}
      footer={
        <>
          <span className="modal-footer-note">Arguments and results are not stored in the audit log.</span>
          {isActiveExecution(execution) && <Button variant="danger" onClick={() => onCancel(execution.id)}>Cancel execution</Button>}
          <Button onClick={onClose}>Close</Button>
        </>
      }
    >
      <div className="stat-strip compact">
        <Stat label="Status" value={<ExecutionStatusBadge status={execution.status} />} />
        <Stat label="Duration" value={formatDuration(execution.durationMs)} />
        <Stat label="Code size" value={formatBytes(execution.codeBytes)} />
        <Stat label="Output size" value={formatBytes(execution.outputBytes)} />
        <Stat label="Error" value={execution.errorCode ?? '—'} />
      </div>

      <h3 className="section-title">Tool calls ({execution.calls.length})</h3>
      {execution.calls.length === 0 ? (
        <p className="muted">This execution made no tool calls.</p>
      ) : (
        <ol className="call-list">
          {execution.calls.map((call, index) => (
            <li key={`${call.toolName}-${index}`} className="call-item">
              <span className="call-index mono">{index + 1}</span>
              <code className="call-name">{call.toolName}</code>
              <Badge tone={call.decision === 'allow' ? 'success' : 'danger'}>{call.decision}</Badge>
              {call.errorCode && <Badge tone="warning">{call.errorCode}</Badge>}
              <span className="call-meta mono">{formatDuration(call.durationMs)} · {formatBytes(call.bytes)}</span>
            </li>
          ))}
        </ol>
      )}
    </Modal>
  )
}
