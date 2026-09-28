import { FileCode2, KeyRound, ScrollText, Search, ShieldCheck, Timer, type LucideIcon } from 'lucide-react'
import type { ConfigSectionKey } from '../../config/types'

export type SectionId = ConfigSectionKey | 'preview'

type SectionMeta = { id: SectionId; label: string; description: string; icon: LucideIcon }

export const SECTIONS: SectionMeta[] = [
  { id: 'server', label: 'Server & access', description: 'Listen address, endpoint and authentication.', icon: KeyRound },
  { id: 'search', label: 'Tool search', description: 'How tools_search finds and returns tools.', icon: Search },
  { id: 'sandbox', label: 'Execution sandbox', description: 'Timeouts, quotas and memory for tools_execute.', icon: Timer },
  { id: 'policy', label: 'Access policy', description: 'Which tools agents may discover and call.', icon: ShieldCheck },
  { id: 'observability', label: 'Logging & storage', description: 'Audit retention and log level.', icon: ScrollText },
  { id: 'preview', label: 'YAML preview', description: 'The generated config.yaml and catalog text.', icon: FileCode2 },
]

export function toSectionId(segment: string | undefined): SectionId {
  return SECTIONS.some(section => section.id === segment) ? (segment as SectionId) : 'server'
}
