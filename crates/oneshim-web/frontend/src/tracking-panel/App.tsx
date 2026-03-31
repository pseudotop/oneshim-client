import { invoke } from '@tauri-apps/api/core'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Brain, Camera, Crosshair, LayoutDashboard, Lightbulb, Settings, WifiOff } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'

interface CaptureState {
  paused: boolean
  indicator_visible: boolean
}

interface ConnectionStatus {
  server: boolean
  llm: boolean
  cli: boolean
}

interface SceneAnalysisResult {
  app_name: string
  window_title: string
  accessibility?: { focused_element?: { role: string; label?: string }; element_count: number }
  ocr_regions: Array<{ text: string }>
  gui_elements: Array<{ role: string }>
  work_type?: string
}

const COLLAPSED_WIDTH = 260
const COLLAPSED_HEIGHT = 36
const EXPANDED_WIDTH = 320
const EXPANDED_HEIGHT = 310

export function App() {
  const [state, setState] = useState<CaptureState>({ paused: false, indicator_visible: true })
  const [conn, setConn] = useState<ConnectionStatus>({ server: false, llm: false, cli: false })
  const [expanded, setExpanded] = useState(false)
  const [feedback, setFeedback] = useState<string | null>(null)
  const [sceneResult, setSceneResult] = useState<SceneAnalysisResult | null>(null)
  const positionSaveTimer = useRef<number | null>(null)
  const feedbackTimer = useRef<number | null>(null)

  const showFeedback = useCallback((msg: string) => {
    setFeedback(msg)
    if (feedbackTimer.current) clearTimeout(feedbackTimer.current)
    feedbackTimer.current = window.setTimeout(() => setFeedback(null), 2000)
  }, [])

  // Explicit drag initiation — backup for data-tauri-drag-region
  const handleDragMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('button')) return
    getCurrentWindow()
      .startDragging()
      .catch((e) => console.debug('startDragging failed:', e))
  }, [])

  useEffect(() => {
    let unlistenCapture: (() => void) | undefined
    let unlistenConn: (() => void) | undefined

    listen<CaptureState>('overlay:capture-state-changed', (e) => {
      setState(e.payload)
    }).then((fn) => {
      unlistenCapture = fn
    })

    listen<ConnectionStatus>('overlay:connection-changed', (e) => {
      setConn(e.payload)
    }).then((fn) => {
      unlistenConn = fn
    })

    invoke<CaptureState>('get_capture_status')
      .then(setState)
      .catch((e) => {
        console.warn('get_capture_status failed:', e)
        showFeedback('Status unavailable')
      })
    invoke<ConnectionStatus>('get_connection_status')
      .then(setConn)
      .catch((e) => {
        console.warn('get_connection_status failed:', e)
        showFeedback('Connection status unavailable')
      })

    // Restore saved position
    invoke<string | null>('get_panel_position')
      .then((pos) => {
        if (pos) {
          const [x, y] = pos.split(',').map(Number)
          if (Number.isFinite(x) && Number.isFinite(y)) {
            getCurrentWindow()
              .setPosition(new LogicalPosition(x, y))
              .catch((e) => console.debug('setPosition failed:', e))
          }
        }
      })
      .catch((e) => console.warn('get_panel_position failed:', e))

    return () => {
      unlistenCapture?.()
      unlistenConn?.()
    }
  }, [showFeedback])

  // Save position on window move (debounced)
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen('tauri://move', (e) => {
      if (positionSaveTimer.current) clearTimeout(positionSaveTimer.current)
      const payload = e.payload as { x?: number; y?: number } | undefined
      if (payload && typeof payload.x === 'number' && typeof payload.y === 'number') {
        positionSaveTimer.current = window.setTimeout(() => {
          invoke('save_panel_position', { x: payload.x, y: payload.y }).catch((e) =>
            console.debug('save_panel_position failed:', e),
          )
        }, 1000)
      }
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  const toggleExpanded = useCallback(async () => {
    const next = !expanded
    setExpanded(next)
    const w = next ? EXPANDED_WIDTH : COLLAPSED_WIDTH
    const h = next ? EXPANDED_HEIGHT : COLLAPSED_HEIGHT
    const win = getCurrentWindow()
    const heightDiff = EXPANDED_HEIGHT - COLLAPSED_HEIGHT

    try {
      const scale = await win.scaleFactor()

      if (next) {
        const pos = await win.outerPosition()
        await win.setPosition(new LogicalPosition(pos.x / scale, pos.y / scale - heightDiff))
        await win.setSize(new LogicalSize(w, h))
      } else {
        await win.setSize(new LogicalSize(w, h))
        const pos = await win.outerPosition()
        await win.setPosition(new LogicalPosition(pos.x / scale, pos.y / scale + heightDiff))
      }
    } catch (e) {
      console.warn('toggleExpanded position failed:', e)
      try {
        await win.setSize(new LogicalSize(w, h))
      } catch (e2) {
        console.warn('toggleExpanded setSize fallback failed:', e2)
      }
    }
  }, [expanded])

  const handleManualCapture = useCallback(async () => {
    try {
      await invoke('trigger_manual_capture')
      showFeedback('Captured')
    } catch (e) {
      console.warn('trigger_manual_capture failed:', e)
      showFeedback('Capture failed')
    }
  }, [showFeedback])

  const handleSceneAnalysis = useCallback(async () => {
    try {
      showFeedback('Analyzing...')
      const result = await invoke<SceneAnalysisResult>('analyze_current_scene')
      setSceneResult(result)
      showFeedback(`${result.app_name} — ${result.accessibility?.element_count ?? 0} elements`)
      // Auto-dismiss scene result after 10s
      setTimeout(() => setSceneResult(null), 10000)
    } catch (e) {
      console.warn('analyze_current_scene failed:', e)
      showFeedback('Analysis failed')
    }
  }, [showFeedback])

  const handleToggleFocus = useCallback(async () => {
    try {
      const status = await invoke<{ active: boolean }>('get_focus_mode_status')
      await invoke('toggle_focus_mode', { active: !status.active, durationMinutes: 25 })
      showFeedback(status.active ? 'Focus off' : 'Focus 25m')
    } catch (e) {
      console.warn('toggle_focus_mode failed:', e)
      showFeedback('Focus toggle failed')
    }
  }, [showFeedback])

  const handleSuggestions = useCallback(async () => {
    try {
      const { emit } = await import('@tauri-apps/api/event')
      await emit('overlay:toggle-suggestions')
      await invoke('toggle_overlay_interactive', { interactive: true })
      showFeedback('Suggestions panel opened')
    } catch (e) {
      console.warn('toggle_overlay_interactive failed:', e)
      showFeedback('Suggestions unavailable')
    }
  }, [showFeedback])

  if (!state.indicator_visible) return null

  const connCount = [conn.server, conn.llm, conn.cli].filter(Boolean).length
  const allConnected = connCount === 3
  const isOffline = connCount === 0

  return (
    <div
      data-tauri-drag-region
      className={`flex select-none flex-col overflow-hidden rounded-xl bg-black/80 text-white text-xs backdrop-blur-md ${state.paused ? '' : 'animate-panel-glow'}`}
      style={
        state.paused
          ? {
              boxShadow: 'inset 0 0 12px 3px rgb(var(--content-muted) / 0.25)',
              border: '1.5px solid rgb(var(--content-muted) / 0.3)',
            }
          : undefined
      }
    >
      {/* Collapsed bar */}
      <div
        role="toolbar"
        data-tauri-drag-region
        onMouseDown={handleDragMouseDown}
        className="flex cursor-move items-center gap-2 px-3 py-2"
      >
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${state.paused ? 'bg-status-connecting' : 'bg-status-connected'}`}
        />
        {!allConnected && (
          <span className="h-2 w-2 shrink-0 rounded-full bg-status-error" title={`${connCount}/3 connected`} />
        )}
        <span data-tauri-drag-region className="flex-1 truncate">
          {state.paused ? 'Paused' : (feedback ?? 'Capturing')}
        </span>

        <button
          type="button"
          onClick={() => invoke('toggle_capture_pause')}
          className="rounded px-1.5 py-0.5 transition-colors hover:bg-white/20"
          title={state.paused ? 'Resume' : 'Pause'}
        >
          {state.paused ? '\u25B6' : '\u23F8'}
        </button>
        <button
          type="button"
          onClick={toggleExpanded}
          className="rounded px-1.5 py-0.5 transition-colors hover:bg-white/20"
          title={expanded ? 'Collapse' : 'Expand'}
        >
          {expanded ? '\u2501' : '\u229E'}
        </button>
        <button
          type="button"
          onClick={() => invoke('set_indicator_visible', { visible: false })}
          className="rounded px-1 py-0.5 transition-colors hover:bg-white/20"
          title="Hide"
        >
          {'\u2715'}
        </button>
      </div>

      {/* Expanded panel */}
      {expanded && (
        <div data-tauri-drag-region className="flex cursor-move flex-col gap-1 border-white/10 border-t px-3 pt-1 pb-3">
          <ActionButton
            icon={<LayoutDashboard size={14} />}
            label="Open Dashboard"
            onClick={() => invoke('show_main_window')}
          />
          <ActionButton icon={<Camera size={14} />} label="Manual Capture" onClick={handleManualCapture} />
          <ActionButton icon={<Brain size={14} />} label="Scene Analysis" onClick={handleSceneAnalysis} />
          <ActionButton icon={<Lightbulb size={14} />} label="AI Suggestions" onClick={handleSuggestions} />
          <ActionButton icon={<Crosshair size={14} />} label="Focus Mode" onClick={handleToggleFocus} />

          {/* Connection status + offline mode indicator */}
          <div data-tauri-drag-region className="mt-2 border-white/10 border-t pt-2">
            {isOffline && (
              <div className="mb-1.5 flex items-center gap-1.5 text-[10px] text-amber-400/80">
                <WifiOff size={10} />
                <span>Offline — local capture + analysis available</span>
              </div>
            )}
            <div data-tauri-drag-region className="flex items-center justify-between text-[10px] text-white/60">
              <div className="flex items-center gap-3">
                <StatusDot connected={conn.server} label="Server" />
                <StatusDot connected={conn.llm} label="LLM" />
                <StatusDot connected={conn.cli} label="CLI" />
              </div>
              <button
                type="button"
                onClick={() => invoke('show_main_window')}
                className="rounded p-0.5 transition-colors hover:bg-white/10"
                title="Open Settings"
              >
                <Settings size={10} />
              </button>
            </div>
          </div>

          {/* Scene analysis result (auto-dismisses after 10s) */}
          {sceneResult && (
            <div className="mt-1 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-[10px]">
              <div className="flex items-center justify-between">
                <span className="truncate font-medium text-white/90">
                  {sceneResult.app_name} — {sceneResult.window_title}
                </span>
                <button
                  type="button"
                  onClick={() => setSceneResult(null)}
                  className="ml-1 text-white/40 hover:text-white/80"
                >
                  &times;
                </button>
              </div>
              <div className="mt-1 flex gap-3 text-white/50">
                <span>{sceneResult.accessibility?.element_count ?? 0} elements</span>
                <span>{sceneResult.ocr_regions.length} OCR</span>
                {sceneResult.work_type && <span>{sceneResult.work_type}</span>}
              </div>
              {sceneResult.accessibility?.focused_element && (
                <div className="mt-0.5 truncate text-white/40">
                  Focus: {sceneResult.accessibility.focused_element.role}
                  {sceneResult.accessibility.focused_element.label &&
                    ` "${sceneResult.accessibility.focused_element.label}"`}
                </div>
              )}
            </div>
          )}
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
  icon: React.ReactNode
  label: string
  onClick?: () => void
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`flex items-center gap-2 rounded-lg px-2 py-1.5 text-left text-white/80 transition-colors ${
        disabled ? 'cursor-not-allowed opacity-40' : 'hover:bg-white/10 active:bg-white/20'
      }`}
      title={disabled ? 'Coming soon' : label}
    >
      <span className="flex w-5 items-center justify-center">{icon}</span>
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
