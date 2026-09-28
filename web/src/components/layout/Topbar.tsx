import { RefreshCw } from 'lucide-react'
import { IconButton } from '../ui'
import { ThemeSwitcher } from './ThemeSwitcher'
import type { ThemePreference } from '../../hooks/useTheme'

type TopbarProps = {
  title: string
  theme: ThemePreference
  onThemeChange: (theme: ThemePreference) => void
  onRefresh: () => void
}

export function Topbar({ title, theme, onThemeChange, onRefresh }: TopbarProps) {
  return (
    <header className="topbar">
      <div className="breadcrumbs">
        <span>Gateway</span>
        <span aria-hidden>/</span>
        <strong>{title}</strong>
      </div>
      <div className="topbar-actions">
        <ThemeSwitcher value={theme} onChange={onThemeChange} />
        <IconButton icon={RefreshCw} label="Refresh data" onClick={onRefresh} />
      </div>
    </header>
  )
}
