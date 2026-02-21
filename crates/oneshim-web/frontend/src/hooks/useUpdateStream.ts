import { useEffect, useRef, useState } from 'react'
import type { UpdateStatus } from '../api/client'

export type UpdateStreamStatus = 'connecting' | 'connected' | 'disconnected' | 'error'

export function useUpdateStream() {
  const [status, setStatus] = useState<UpdateStreamStatus>('disconnected')
  const [latest, setLatest] = useState<UpdateStatus | null>(null)
  const esRef = useRef<EventSource | null>(null)
  const retryRef = useRef<number | null>(null)
  const retries = useRef(0)

  useEffect(() => {
    const connect = () => {
      if (esRef.current) {
        esRef.current.close()
      }
      setStatus('connecting')
      const es = new EventSource('/api/update/stream')
      esRef.current = es

      es.onopen = () => {
        retries.current = 0
        setStatus('connected')
      }

      es.addEventListener('update_status', (event) => {
        try {
          const parsed = JSON.parse((event as MessageEvent).data) as UpdateStatus
          setLatest(parsed)
        } catch {
          // noop
        }
      })

      es.onerror = () => {
        setStatus('error')
        es.close()
        if (retries.current < 10) {
          retries.current += 1
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
    }
  }, [])

  return { status, latest }
}
