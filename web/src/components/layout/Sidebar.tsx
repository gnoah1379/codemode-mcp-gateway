import { Code2, LogOut } from 'lucide-react'
import { NAV_ITEMS, type PageId } from '../../app/navigation'
import { IconButton, StatusDot } from '../ui'

type SidebarProps = {
  active: PageId
  onNavigate: (page: PageId) => void
  activeExecutions: number
  connected: boolean
  onSignOut: () => void
}

export function Sidebar({ active, onNavigate, activeExecutions, connected, onSignOut }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark"><Code2 size={18} aria-hidden /></span>
        <div className="brand-text">
          <strong>Code Mode</strong>
          <span>MCP Gateway</span>
        </div>
      </div>

      <nav className="nav" aria-label="Main">
        {NAV_ITEMS.map(({ id, label, icon: Icon }) => (
          <a
            key={id}
            href={`#/${id}`}
            className={`nav-item${active === id ? ' active' : ''}`}
            aria-current={active === id ? 'page' : undefined}
            onClick={event => {
              event.preventDefault()
              onNavigate(id)
            }}
          >
            <Icon size={18} aria-hidden />
            <span>{label}</span>
            {id === 'executions' && activeExecutions > 0 && <b className="nav-count">{activeExecutions}</b>}
          </a>
        ))}
      </nav>

      <div className="sidebar-footer">
        <div className="connection">
          <StatusDot tone={connected ? 'success' : 'neutral'} />
          <span>{connected ? 'Connected' : 'Connecting…'}</span>
        </div>
        <IconButton icon={LogOut} label="Sign out" onClick={onSignOut} />
      </div>
    </aside>
  )
}
