import { useCallback, useEffect, useState } from 'react'

/**
 * Minimal hash router: `#/configuration/policy` → `['configuration', 'policy']`.
 * The hash keeps the console deep-linkable without server-side routing.
 */
function readSegments(): string[] {
  return window.location.hash.replace(/^#\/?/, '').split('/').filter(Boolean)
}

export function useRoute() {
  const [segments, setSegments] = useState<string[]>(readSegments)

  useEffect(() => {
    const onChange = () => setSegments(readSegments())
    window.addEventListener('hashchange', onChange)
    return () => window.removeEventListener('hashchange', onChange)
  }, [])

  const navigate = useCallback((...next: string[]) => {
    window.location.hash = `/${next.join('/')}`
  }, [])

  return { segments, navigate }
}
