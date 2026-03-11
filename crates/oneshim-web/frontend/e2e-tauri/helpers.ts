// e2e-tauri/helpers.ts

/**
 * Tauri IPC 커맨드 호출 — window.__TAURI_INTERNALS__.invoke() 래핑
 * WebdriverIO의 executeAsync로 브라우저 컨텍스트에서 실행
 */
export async function invokeIpc<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  const result = await browser.executeAsync(
    (cmd: string, cmdArgs: Record<string, unknown> | undefined, done: (r: { ok: boolean; data?: unknown; error?: string }) => void) => {
      const tauri = (window as any).__TAURI_INTERNALS__
      if (!tauri) {
        done({ ok: false, error: 'TAURI_INTERNALS not available' })
        return
      }
      tauri
        .invoke(cmd, cmdArgs || {})
        .then((data: unknown) => done({ ok: true, data }))
        .catch((err: unknown) => done({ ok: false, error: String(err) }))
    },
    command,
    args
  )
  if (!result.ok) {
    throw new Error(`IPC ${command} failed: ${result.error}`)
  }
  return result.data as T
}

/**
 * SSE 이벤트 수신 대기 — /api/stream에서 특정 이벤트 타입 캡처
 * WebView 내부의 EventSource를 사용하므로 CSP connect-src 범위 내에서 동작
 */
export async function waitForSseEvent(
  eventType: string,
  timeoutMs = 10000
): Promise<Record<string, unknown>> {
  const result = await browser.executeAsync(
    (type: string, timeout: number, done: (r: { ok: boolean; data?: unknown; error?: string }) => void) => {
      const port = (window as any).__ONESHIM_WEB_PORT__ || 10090
      const es = new EventSource(`http://127.0.0.1:${port}/api/stream`)
      const timer = setTimeout(() => {
        es.close()
        done({ ok: false, error: `SSE ${type} timeout after ${timeout}ms` })
      }, timeout)
      es.addEventListener(type, (e: MessageEvent) => {
        clearTimeout(timer)
        es.close()
        try {
          done({ ok: true, data: JSON.parse(e.data) })
        } catch {
          done({ ok: true, data: e.data })
        }
      })
      es.onerror = () => {
        clearTimeout(timer)
        es.close()
        done({ ok: false, error: 'SSE connection error' })
      }
    },
    eventType,
    timeoutMs
  )
  if (!result.ok) {
    throw new Error(result.error)
  }
  return result.data as Record<string, unknown>
}

/**
 * IPC 응답 타입 정의
 */
export interface MetricsResponse {
  agent_cpu: number
  agent_memory_mb: number
  system_cpu: number
  system_memory_used_mb: number
  system_memory_total_mb: number
}

export interface UpdateStatusResponse {
  phase: string
  message?: string
  latest_version?: string
}
