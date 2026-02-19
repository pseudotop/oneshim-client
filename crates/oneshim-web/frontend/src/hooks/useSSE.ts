import { useEffect, useRef, useState, useCallback } from 'react'

// 실시간 이벤트 타입 (백엔드와 동일)
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

// SSE 연결 상태
export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error'

// SSE 훅 옵션
interface UseSSEOptions {
  // 자동 재연결 여부 (기본: true)
  autoReconnect?: boolean
  // 재연결 딜레이 (ms, 기본: 3000)
  reconnectDelay?: number
  // 최대 재연결 시도 횟수 (기본: 10)
  maxRetries?: number
}

// SSE 훅 반환값
interface UseSSEResult {
  // 연결 상태
  status: ConnectionStatus
  // 최근 메트릭 업데이트
  latestMetrics: MetricsUpdate | null
  // 최근 프레임 업데이트
  latestFrame: FrameUpdate | null
  // 유휴 상태
  idleState: IdleUpdate | null
  // 메트릭 히스토리 (최근 N개)
  metricsHistory: MetricsUpdate[]
  // 수동 연결
  connect: () => void
  // 수동 연결 해제
  disconnect: () => void
}

const SSE_URL = '/api/stream'
const MAX_HISTORY_SIZE = 60 // 60개 데이터 포인트 (약 5분)

/**
 * SSE 실시간 이벤트 훅
 *
 * 서버에서 전송하는 실시간 이벤트를 수신하고 상태를 관리합니다.
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

  // 이벤트 핸들러
  const handleEvent = useCallback((event: MessageEvent) => {
    try {
      const data = JSON.parse(event.data) as RealtimeEvent

      switch (data.type) {
        case 'metrics':
          setLatestMetrics(data.data)
          setMetricsHistory((prev) => {
            const newHistory = [...prev, data.data]
            // 최대 크기 유지
            if (newHistory.length > MAX_HISTORY_SIZE) {
              return newHistory.slice(-MAX_HISTORY_SIZE)
            }
            return newHistory
          })
          break
        case 'frame':
          setLatestFrame(data.data)
          break
        case 'idle':
          setIdleState(data.data)
          break
        case 'ping':
          // Heartbeat - 연결 유지 확인
          break
      }
    } catch {
      console.error('SSE 이벤트 파싱 오류:', event.data)
    }
  }, [])

  // handleEvent를 ref로 유지하여 connect가 재생성되지 않도록 함
  const handleEventRef = useRef(handleEvent)
  handleEventRef.current = handleEvent

  // 연결 함수
  const connect = useCallback(() => {
    // 이미 연결 중이면 스킵
    if (eventSourceRef.current?.readyState === EventSource.OPEN) {
      return
    }

    // 기존 연결 정리
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

    // 각 이벤트 타입별 리스너 (ref를 통해 최신 핸들러 참조)
    const handler = (event: MessageEvent) => handleEventRef.current(event)
    eventSource.addEventListener('metrics', handler)
    eventSource.addEventListener('frame', handler)
    eventSource.addEventListener('idle', handler)
    eventSource.addEventListener('ping', handler)

    eventSource.onerror = () => {
      setStatus('error')
      eventSource.close()

      // 자동 재연결
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

  // 연결 해제 함수
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

  // 컴포넌트 마운트 시 연결
  useEffect(() => {
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
