import { useEffect, useRef, useState, useCallback } from 'react'
import { isStandaloneModeEnabled } from '../api/standalone'

export interface MetricsUpdate {
  timestamp: string
  cpu_usage: number
  memory_percent: number
  memory_used: number
  memory_total: number
}

export interface FrameUpdate {
  id: number
  timestamp: string
  app_name: string
  window_title: string
  importance: number
}

export interface IdleUpdate {
  is_idle: boolean
  idle_secs: number
}

export type RealtimeEvent =
  | { type: 'metrics'; data: MetricsUpdate }
  | { type: 'frame'; data: FrameUpdate }
  | { type: 'idle'; data: IdleUpdate }
  | { type: 'ping' }

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error'

interface UseSSEOptions {
  autoReconnect?: boolean
  reconnectDelay?: number
  maxRetries?: number
}

interface UseSSEResult {
  status: ConnectionStatus
  latestMetrics: MetricsUpdate | null
  latestFrame: FrameUpdate | null
  idleState: IdleUpdate | null
  metricsHistory: MetricsUpdate[]
  connect: () => void
  disconnect: () => void
}

const SSE_URL = '/api/stream'
const MAX_HISTORY_SIZE = 60 // 60items data ( 5min)
/**
 *
 */
export function useSSE(options: UseSSEOptions = {}): UseSSEResult {
  const {
    autoReconnect = true,
    reconnectDelay = 3000,
    maxRetries = 10,
  } = options

  const [status, setStatus] = useState<ConnectionStatus>('disconnected')
  const [latestMetrics, setLatestMetrics] = useState<MetricsUpdate | null>(null)
  const [latestFrame, setLatestFrame] = useState<FrameUpdate | null>(null)
  const [idleState, setIdleState] = useState<IdleUpdate | null>(null)
  const [metricsHistory, setMetricsHistory] = useState<MetricsUpdate[]>([])

  const eventSourceRef = useRef<EventSource | null>(null)
  const retryCountRef = useRef(0)
  const reconnectTimeoutRef = useRef<number | null>(null)

  const handleEvent = useCallback((event: MessageEvent) => {
    try {
      const data = JSON.parse(event.data) as RealtimeEvent

      switch (data.type) {
        case 'metrics':
          setLatestMetrics(data.data)
          setMetricsHistory((prev) => {
            if (prev.length >= MAX_HISTORY_SIZE) {
              return [...prev.slice(1), data.data]
            }
            return [...prev, data.data]
          })
          break
        case 'frame':
          setLatestFrame(data.data)
          break
        case 'idle':
          setIdleState(data.data)
          break
        case 'ping':
          break
      }
    } catch {
      console.error('SSE event parse error:', event.data)
    }
  }, [])

  const handleEventRef = useRef(handleEvent)
  handleEventRef.current = handleEvent

  const connect = useCallback(() => {
    if (eventSourceRef.current?.readyState === EventSource.OPEN) {
      return
    }

    if (eventSourceRef.current) {
      eventSourceRef.current.close()
    }

    setStatus('connecting')
    const eventSource = new EventSource(SSE_URL)
    eventSourceRef.current = eventSource

    eventSource.onopen = () => {
      setStatus('connected')
      retryCountRef.current = 0
    }

    const handler = (event: MessageEvent) => handleEventRef.current(event)
    eventSource.addEventListener('metrics', handler)
    eventSource.addEventListener('frame', handler)
    eventSource.addEventListener('idle', handler)
    eventSource.addEventListener('ping', handler)

    eventSource.onerror = () => {
      setStatus('error')
      eventSource.close()

      if (autoReconnect && retryCountRef.current < maxRetries) {
        retryCountRef.current++
        reconnectTimeoutRef.current = window.setTimeout(() => {
          connect()
        }, reconnectDelay)
      } else {
        setStatus('disconnected')
      }
    }
  }, [autoReconnect, reconnectDelay, maxRetries])

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current)
      reconnectTimeoutRef.current = null
    }

    if (eventSourceRef.current) {
      eventSourceRef.current.close()
      eventSourceRef.current = null
    }

    setStatus('disconnected')
    retryCountRef.current = 0
  }, [])

  useEffect(() => {
    if (isStandaloneModeEnabled()) {
      setStatus('disconnected')
      return () => {}
    }
    connect()
    return () => {
      disconnect()
    }
  }, [connect, disconnect])

  return {
    status,
    latestMetrics,
    latestFrame,
    idleState,
    metricsHistory,
    connect,
    disconnect,
  }
}
