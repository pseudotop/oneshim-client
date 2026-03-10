import { DEFAULT_WEB_PORT } from '../constants'
import { IS_TAURI } from './platform'

/**
 * In Tauri webview, the frontend is served via tauri:// protocol.
 * Relative URLs like /api/stream won't reach the Axum backend at 127.0.0.1.
 * Use absolute URLs when running inside Tauri.
 *
 * The web port is resolved in this order:
 * 1. window.__ONESHIM_WEB_PORT__ (injected by setup.rs via eval before page loads)
 * 2. Tauri IPC get_web_port command (async, updates URLs after init)
 * 3. DEFAULT_WEB_PORT fallback (10090)
 */
declare global {
  interface Window {
    __ONESHIM_WEB_PORT__?: number
  }
}

let resolvedPort =
  (typeof window !== 'undefined' && window.__ONESHIM_WEB_PORT__) || DEFAULT_WEB_PORT

function buildApiUrl(): string {
  return IS_TAURI ? `http://127.0.0.1:${resolvedPort}/api` : '/api'
}

function buildSseUrl(): string {
  return IS_TAURI ? `http://127.0.0.1:${resolvedPort}/api/stream` : '/api/stream'
}

function buildUpdateStreamUrl(): string {
  return IS_TAURI ? `http://127.0.0.1:${resolvedPort}/api/update/stream` : '/api/update/stream'
}

// Mutable exports — updated when Tauri IPC resolves the actual port
export let API_BASE_URL = buildApiUrl()
export let SSE_STREAM_URL = buildSseUrl()
export let UPDATE_STREAM_URL = buildUpdateStreamUrl()

// Async port resolution via Tauri IPC (runs once at startup)
if (IS_TAURI) {
  import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke<number>('get_web_port'))
    .then((port) => {
      if (port && port !== resolvedPort) {
        resolvedPort = port
        API_BASE_URL = buildApiUrl()
        SSE_STREAM_URL = buildSseUrl()
        UPDATE_STREAM_URL = buildUpdateStreamUrl()
      }
    })
    .catch(() => {
      /* fallback to eval-injected or default port */
    })
}
