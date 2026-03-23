import { useState, useEffect, useCallback, useRef } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'

interface CaptureState {
  paused: boolean
  indicator_visible: boolean
}

interface ConnectionStatus {
  server: boolean
  llm: boolean
  cli: boolean
}

const COLLAPSED_WIDTH = 260
const COLLAPSED_HEIGHT = 36
const EXPANDED_WIDTH = 320
const EXPANDED_HEIGHT = 260

export function App() {
  const [state, setState] = useState<CaptureState>({ paused: false, indicator_visible: true })
  const [conn, setConn] = useState<ConnectionStatus>({ server: false, llm: false, cli: false })
  const [expanded, setExpanded] = useState(false)
  const positionSaveTimer = useRef<number | null>(null)

  useEffect(() => {
    let unlistenCapture: (() => void) | undefined
    let unlistenConn: (() => void) | undefined

    listen<CaptureState>('overlay:capture-state-changed', (e) => {
      setState(e.payload)
    }).then((fn) => { unlistenCapture = fn })

    listen<ConnectionStatus>('overlay:connection-changed', (e) => {
      setConn(e.payload)
    }).then((fn) => { unlistenConn = fn })

    invoke<CaptureState>('get_capture_status').then(setState).catch(() => {})
    invoke<ConnectionStatus>('get_connection_status').then(setConn).catch(() => {})

    // Restore saved position
    invoke<string | null>('get_panel_position').then((pos) => {
      if (pos) {
        const [x, y] = pos.split(',').map(Number)
        if (Number.isFinite(x) && Number.isFinite(y)) {
          getCurrentWindow().setPosition(new LogicalPosition(x, y)).catch(() => {})
        }
      }
    }).catch(() => {})

    return () => { unlistenCapture?.(); unlistenConn?.() }
  }, [])

  // Save position on window move (debounced)
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen('tauri://move', (e) => {
      if (positionSaveTimer.current) clearTimeout(positionSaveTimer.current)
      const payload = e.payload as { x?: number; y?: number } | undefined
      if (payload && typeof payload.x === 'number' && typeof payload.y === 'number') {
        positionSaveTimer.current = window.setTimeout(() => {
          invoke('save_panel_position', { x: payload.x, y: payload.y }).catch(() => {})
        }, 1000)
      }
    }).then((fn) => { unlisten = fn })
    return () => unlisten?.()
  }, [])

  const toggleExpanded = useCallback(async () => {
    const next = !expanded
    setExpanded(next)
    try {
      await getCurrentWindow().setSize(new LogicalSize(
        next ? EXPANDED_WIDTH : COLLAPSED_WIDTH,
        next ? EXPANDED_HEIGHT : COLLAPSED_HEIGHT,
      ))
    } catch {
      // setSize may fail on non-resizable windows — degrade gracefully
    }
  }, [expanded])

  if (!state.indicator_visible) return null

  const connCount = [conn.server, conn.llm, conn.cli].filter(Boolean).length
  const allConnected = connCount === 3

  return (
    <div className="flex flex-col bg-black/80 backdrop-blur-md text-white text-xs select-none rounded-xl overflow-hidden shadow-2xl">
      {/* Collapsed bar */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 px-3 py-2 cursor-move"
      >
        <span
          className={`h-2 w-2 rounded-full shrink-0 ${
            state.paused ? 'bg-yellow-400' : 'bg-green-400 animate-pulse'
          }`}
        />
        {!allConnected && (
          <span
            className="h-2 w-2 rounded-full shrink-0 bg-red-400"
            title={`${connCount}/3 connected`}
          />
        )}
        <span data-tauri-drag-region className="flex-1 truncate">
          {state.paused ? 'Paused' : 'Capturing'}
        </span>

        <button
          onClick={() => invoke('toggle_capture_pause')}
          className="px-1.5 py-0.5 rounded hover:bg-white/20 transition-colors"
          title={state.paused ? 'Resume' : 'Pause'}
        >
          {state.paused ? '\u25B6' : '\u23F8'}
        </button>
        <button
          onClick={toggleExpanded}
          className="px-1.5 py-0.5 rounded hover:bg-white/20 transition-colors"
          title={expanded ? 'Collapse' : 'Expand'}
        >
          {expanded ? '\u2501' : '\u229E'}
        </button>
        <button
          onClick={() => invoke('set_indicator_visible', { visible: false })}
          className="px-1 py-0.5 rounded hover:bg-white/20 transition-colors"
          title="Hide"
        >
          {'\u2715'}
        </button>
      </div>

      {/* Expanded panel */}
      {expanded && (
        <div className="flex flex-col gap-1 px-3 pb-3 pt-1 border-t border-white/10">
          <ActionButton
            icon="📊"
            label="Open Dashboard"
            onClick={() => invoke('show_main_window')}
          />
          <ActionButton icon="📷" label="Manual Capture" disabled />
          <ActionButton icon="🧠" label="Scene Analysis" disabled />
          <ActionButton icon="💡" label="AI Suggestions" disabled />
          <ActionButton icon="🎯" label="Focus Mode" disabled />

          {/* Connection status detail */}
          <div className="mt-2 pt-2 border-t border-white/10">
            <div className="flex items-center gap-3 text-[10px] text-white/60">
              <StatusDot connected={conn.server} label="Server" />
              <StatusDot connected={conn.llm} label="LLM" />
              <StatusDot connected={conn.cli} label="CLI" />
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function ActionButton({
  icon,
  label,
  onClick,
  disabled,
}: {
  icon: string
  label: string
  onClick?: () => void
  disabled?: boolean
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex items-center gap-2 px-2 py-1.5 rounded-lg transition-colors text-left ${
        disabled
          ? 'opacity-40 cursor-not-allowed'
          : 'hover:bg-white/10 active:bg-white/20'
      }`}
      title={disabled ? 'Coming soon' : label}
    >
      <span className="w-5 text-center">{icon}</span>
      <span>{label}</span>
    </button>
  )
}

function StatusDot({ connected, label }: { connected: boolean; label: string }) {
  return (
    <span className="flex items-center gap-1">
      <span className={`h-1.5 w-1.5 rounded-full ${connected ? 'bg-green-400' : 'bg-red-400'}`} />
      {label}
    </span>
  )
}
