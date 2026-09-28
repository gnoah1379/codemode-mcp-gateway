import { useCallback, useEffect, useRef, useState } from 'react'
import { GATEWAY_EVENTS_URL, gatewayApi } from '../api/gateway'
import { isUnauthorized } from '../api/client'
import { EMPTY_METRICS, type Execution, type Metrics, type UpstreamStatus } from '../api/types'

const REFRESH_EVENTS = [
  'execution.queued',
  'execution.started',
  'execution.finished',
  'execution.call',
  'upstreams.changed',
  'resync_required',
] as const

/**
 * Live gateway status: upstream health, recent executions and metrics.
 * Subscribes to the server-sent event stream while signed in and refetches on every event.
 * `configEpoch` increments whenever the gateway reports a configuration change.
 */
export function useGatewayData(enabled: boolean, onUnauthorized: () => void) {
  const [upstreams, setUpstreams] = useState<UpstreamStatus[]>([])
  const [executions, setExecutions] = useState<Execution[]>([])
  const [metrics, setMetrics] = useState<Metrics>(EMPTY_METRICS)
  const [dataEpoch, setDataEpoch] = useState(0)
  const [configEpoch, setConfigEpoch] = useState(0)

  const onUnauthorizedRef = useRef(onUnauthorized)
  useEffect(() => {
    onUnauthorizedRef.current = onUnauthorized
  })

  const refresh = useCallback(async () => {
    try {
      const [nextUpstreams, nextExecutions, nextMetrics] = await Promise.all([
        gatewayApi.upstreams(),
        gatewayApi.executions(),
        gatewayApi.metrics(),
      ])
      setUpstreams(nextUpstreams)
      setExecutions(nextExecutions)
      setMetrics(nextMetrics)
      setDataEpoch(value => value + 1)
    } catch (error) {
      if (isUnauthorized(error)) onUnauthorizedRef.current()
      throw error
    }
  }, [])

  useEffect(() => {
    if (!enabled) return
    void refresh().catch(() => undefined)

    const events = new EventSource(GATEWAY_EVENTS_URL)
    const onEvent = () => void refresh().catch(() => undefined)
    const onConfigChanged = () => {
      setConfigEpoch(value => value + 1)
      onEvent()
    }
    events.onmessage = onEvent
    for (const name of REFRESH_EVENTS) events.addEventListener(name, onEvent)
    events.addEventListener('config.changed', onConfigChanged)
    return () => events.close()
  }, [enabled, refresh])

  return { upstreams, executions, metrics, refresh, dataEpoch, configEpoch }
}

export type GatewayData = ReturnType<typeof useGatewayData>
