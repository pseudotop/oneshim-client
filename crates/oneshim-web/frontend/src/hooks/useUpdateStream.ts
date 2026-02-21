import { useEffect, useRef, useState } from 'react'
import type { UpdateStatus } from '../api/client'

export type UpdateStreamStatus = 'connecting' | 'connected' | 'disconnected' | 'error'

export function useUpdateStream() {
  const [status, setStatus] = useState<UpdateStreamStatus>('disconnected')
  const [latest, setLatest] = useState<UpdateStatus | null>(null)
  const [lastEventAt, setLastEventAt] = useState<number | null>(null)
  const [lastError, setLastError] = useState<string | null>(null)
  const [retryCount, setRetryCount] = useState(0)
  const esRef = useRef<EventSource | null>(null)
  const retryRef = useRef<number | null>(null)
  const retries = useRef(0)

  useEffect(() => {
    const connect = () => {
      if (esRef.current) {
        esRef.current.close()
      }
      setStatus('connecting')
      setLastError(null)
      const es = new EventSource('/api/update/stream')
      esRef.current = es

      es.onopen = () => {
        retries.current = 0
        setRetryCount(0)
        setStatus('connected')
      }

      es.addEventListener('update_status', (event) => {
        try {
          const parsed = JSON.parse((event as MessageEvent).data) as UpdateStatus
          setLatest(parsed)
          setLastEventAt(Date.now())
          setLastError(null)
        } catch {
          setLastError('stream_parse_error')
        }
      })

      es.onerror = () => {
        setStatus('error')
        setLastError('stream_connection_error')
        es.close()
        if (retries.current < 10) {
          retries.current += 1
          setRetryCount(retries.current)
          retryRef.current = window.setTimeout(connect, 2000)
        } else {
          setStatus('disconnected')
        }
      }
    }

    connect()
    return () => {
      if (retryRef.current) {
        clearTimeout(retryRef.current)
      }
      if (esRef.current) {
        esRef.current.close()
      }
      setStatus('disconnected')
      setLastError(null)
      setRetryCount(0)
    }
  }, [])

  return { status, latest, lastEventAt, lastError, retryCount }
}
