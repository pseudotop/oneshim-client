import { useState, useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

interface CaptureState {
  paused: boolean
  indicator_visible: boolean
}

interface ConnectionStatus {
  server: boolean
  llm: boolean
  cli: boolean
}

export function App() {
  const [state, setState] = useState<CaptureState>({ paused: false, indicator_visible: true })
  const [conn, setConn] = useState<ConnectionStatus>({ server: false, llm: false, cli: false })

  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<CaptureState>('overlay:capture-state-changed', (e) => {
      setState(e.payload)
    }).then((fn) => { unlisten = fn })

    invoke<CaptureState>('get_capture_status').then(setState).catch(() => {})
    invoke<ConnectionStatus>('get_connection_status').then(setConn).catch(() => {})

    return () => unlisten?.()
  }, [])

  if (!state.indicator_visible) return null

  return (
    <div className="flex items-center gap-2 rounded-full bg-black/70 backdrop-blur-sm px-3 py-1 text-white text-xs select-none">
      <span className={`h-2 w-2 rounded-full ${state.paused ? 'bg-yellow-400' : 'bg-green-400 animate-pulse'}`} />
      {(!conn.server || !conn.llm || !conn.cli) && (
        <span className="h-2 w-2 rounded-full bg-red-400" title="Some services disconnected" />
      )}
      <span>{state.paused ? 'Paused' : 'Capturing'}</span>
      <button
        onClick={() => invoke('toggle_capture_pause')}
        className="ml-1 px-1.5 py-0.5 rounded hover:bg-white/20 transition-colors"
        title={state.paused ? 'Resume' : 'Pause'}
      >
        {state.paused ? '\u25B6' : '\u23F8'}
      </button>
      <button
        onClick={() => invoke('set_indicator_visible', { visible: false })}
        className="px-1 py-0.5 rounded hover:bg-white/20 transition-colors"
        title="Hide indicator"
      >
        {'\u2715'}
      </button>
    </div>
  )
}
