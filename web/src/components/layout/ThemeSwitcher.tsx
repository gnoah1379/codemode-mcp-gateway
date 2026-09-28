import { Monitor, Moon, Sun } from 'lucide-react'
import { SegmentedControl, type Segment } from '../ui'
import type { ThemePreference } from '../../hooks/useTheme'

const OPTIONS: Segment<ThemePreference>[] = [
  { value: 'light', label: 'Light theme', icon: Sun },
  { value: 'system', label: 'Match system', icon: Monitor },
  { value: 'dark', label: 'Dark theme', icon: Moon },
]

export function ThemeSwitcher({ value, onChange }: { value: ThemePreference; onChange: (value: ThemePreference) => void }) {
  return <SegmentedControl label="Color theme" size="sm" iconOnly value={value} options={OPTIONS} onChange={onChange} />
}
