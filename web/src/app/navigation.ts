import { Activity, GitBranch, LayoutDashboard, Search, Settings2, type LucideIcon } from 'lucide-react'

export type PageId = 'dashboard' | 'upstreams' | 'tools' | 'configuration' | 'executions'

export type NavItem = { id: PageId; label: string; icon: LucideIcon }

export const NAV_ITEMS: NavItem[] = [
  { id: 'dashboard', label: 'Overview', icon: LayoutDashboard },
  { id: 'upstreams', label: 'Upstreams', icon: GitBranch },
  { id: 'tools', label: 'Tools', icon: Search },
  { id: 'executions', label: 'Executions', icon: Activity },
  { id: 'configuration', label: 'Settings', icon: Settings2 },
]

export function toPageId(segment: string | undefined): PageId {
  return NAV_ITEMS.some(item => item.id === segment) ? (segment as PageId) : 'dashboard'
}

export function pageLabel(id: PageId): string {
  return NAV_ITEMS.find(item => item.id === id)?.label ?? 'Overview'
}
