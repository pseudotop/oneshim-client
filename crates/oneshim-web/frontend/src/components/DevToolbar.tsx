/**
 * Dev-only debug toolbar. Only rendered when import.meta.env.DEV is true.
 * Provides quick toggles for common dev tasks without needing Console access.
 */
import { useState } from 'react'

const STANDALONE_KEY = 'oneshim-web-standalone-mode'

function isStandalone(): boolean {
  return localStorage.getItem(STANDALONE_KEY) === '1'
}

export function DevToolbar() {
  const [open, setOpen] = useState(false)
  const [standalone, setStandalone] = useState(isStandalone)

  if (!import.meta.env.DEV) return null

  const toggleStandalone = () => {
    const next = !standalone
    localStorage.setItem(STANDALONE_KEY, next ? '1' : '0')
    setStandalone(next)
    location.reload()
  }

  const clearStorage = () => {
    localStorage.clear()
    location.reload()
  }

  const apiBase = window.__ONESHIM_WEB_PORT__
    ? `http://127.0.0.1:${window.__ONESHIM_WEB_PORT__}`
    : 'http://127.0.0.1:10090'

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="fixed bottom-12 right-4 z-[9999] bg-yellow-500 text-black text-xs font-bold px-2 py-1 rounded-full shadow-lg opacity-60 hover:opacity-100"
        title="Open Dev Toolbar"
      >
        DEV
      </button>
    )
  }

  return (
    <div className="fixed bottom-12 right-4 z-[9999] bg-gray-900 text-white text-xs rounded-lg shadow-2xl p-3 w-72 border border-yellow-500/50">
      <div className="flex justify-between items-center mb-2">
        <span className="font-bold text-yellow-400">Dev Toolbar</span>
        <button onClick={() => setOpen(false)} className="text-gray-400 hover:text-white">✕</button>
      </div>

      <div className="space-y-2">
        <div className="flex justify-between items-center">
          <span>Standalone Mock</span>
          <button
            onClick={toggleStandalone}
            className={`px-2 py-0.5 rounded text-xs font-bold ${
              standalone ? 'bg-red-600' : 'bg-green-600'
            }`}
          >
            {standalone ? 'ON (mock)' : 'OFF (real API)'}
          </button>
        </div>

        <div className="flex justify-between items-center">
          <span>API Base</span>
          <span className="text-gray-400 truncate ml-2">{apiBase}</span>
        </div>

        <div className="flex justify-between items-center">
          <span>Tauri</span>
          <span className="text-gray-400">
            {'__TAURI_INTERNALS__' in window ? 'Yes' : 'No'}
          </span>
        </div>

        <div className="flex justify-between items-center">
          <span>Port</span>
          <span className="text-gray-400">{window.__ONESHIM_WEB_PORT__ ?? 'default'}</span>
        </div>

        <hr className="border-gray-700" />

        <div className="flex gap-2">
          <button
            onClick={clearStorage}
            className="flex-1 bg-red-800 hover:bg-red-700 px-2 py-1 rounded text-xs"
          >
            Clear Storage
          </button>
          <button
            onClick={() => location.reload()}
            className="flex-1 bg-blue-800 hover:bg-blue-700 px-2 py-1 rounded text-xs"
          >
            Reload
          </button>
        </div>
      </div>
    </div>
  )
}
