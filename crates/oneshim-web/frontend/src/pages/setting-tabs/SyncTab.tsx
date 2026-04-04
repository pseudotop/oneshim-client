import { useCallback, useEffect, useState } from 'react'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface SyncStatus {
  enabled: boolean
  device_id: string
  device_name: string
}

interface SyncResult {
  applied: number
  skipped: number
  tombstoned: number
}

interface SyncPeer {
  device_id: string
  device_name: string
  last_sync_at: string
}

async function tauriInvoke<T>(cmd: string): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd)
}

export default function SyncTab() {
  const [status, setStatus] = useState<SyncStatus | null>(null)
  const [peers, setPeers] = useState<SyncPeer[]>([])
  const [syncing, setSyncing] = useState(false)
  const [lastResult, setLastResult] = useState<SyncResult | null>(null)
  const [error, setError] = useState<string | null>(null)

  const fetchStatus = useCallback(async () => {
    try {
      const s = await tauriInvoke<SyncStatus>('get_sync_status')
      setStatus(s)
      if (s.enabled) {
        const p = await tauriInvoke<SyncPeer[]>('discover_sync_peers')
        setPeers(p)
      }
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    fetchStatus()
  }, [fetchStatus])

  const handleSync = async () => {
    setSyncing(true)
    setError(null)
    try {
      const result = await tauriInvoke<SyncResult>('trigger_sync_cycle')
      setLastResult(result)
      await fetchStatus()
    } catch (e) {
      setError(String(e))
    } finally {
      setSyncing(false)
    }
  }

  if (!status) {
    return (
      <div className={cn('text-sm', colors.text.tertiary)}>Loading sync status...</div>
    )
  }

  if (!status.enabled) {
    return (
      <div className="space-y-4">
        <h2 className={cn(typography.h2, colors.text.primary)}>Cross-Device Sync</h2>
        <div className={cn('rounded-lg border p-4', colors.surface.muted)}>
          <p className={cn('text-sm', colors.text.secondary)}>
            Sync is not enabled. To activate cross-device sync:
          </p>
          <ol className={cn('mt-2 list-decimal pl-5 text-sm space-y-1', colors.text.tertiary)}>
            <li>
              Set <code className="rounded bg-gray-200 px-1 dark:bg-gray-700">sync.enabled = true</code> in config
            </li>
            <li>
              Set the <code className="rounded bg-gray-200 px-1 dark:bg-gray-700">ONESHIM_SYNC_PASSPHRASE</code> environment variable
            </li>
            <li>Choose a transport (File, Remote, or LAN) in sync settings</li>
            <li>Restart the application</li>
          </ol>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <h2 className={cn(typography.h2, colors.text.primary)}>Cross-Device Sync</h2>

      {/* Device Info */}
      <div className={cn('rounded-lg border p-4', colors.surface.elevated)}>
        <h3 className={cn('text-sm font-semibold mb-2', colors.text.primary)}>This Device</h3>
        <div className="grid grid-cols-2 gap-2 text-sm">
          <span className={colors.text.tertiary}>Name</span>
          <span className={colors.text.primary}>{status.device_name}</span>
          <span className={colors.text.tertiary}>ID</span>
          <span className={cn('font-mono text-xs', colors.text.secondary)}>
            {status.device_id.slice(0, 12)}...
          </span>
        </div>
      </div>

      {/* Sync Action */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSync}
          disabled={syncing}
          className={cn(
            'rounded-md px-4 py-2 text-sm font-medium text-white',
            syncing ? 'bg-gray-400 cursor-not-allowed' : 'bg-blue-600 hover:bg-blue-700',
          )}
        >
          {syncing ? 'Syncing...' : 'Sync Now'}
        </button>
        {lastResult && (
          <span className={cn('text-sm', colors.text.tertiary)}>
            Applied: {lastResult.applied}, Skipped: {lastResult.skipped}
          </span>
        )}
      </div>

      {error && (
        <div className="rounded-md bg-red-50 p-3 text-sm text-red-700 dark:bg-red-900/20 dark:text-red-300">
          {error}
        </div>
      )}

      {/* Peers */}
      <div>
        <h3 className={cn('text-sm font-semibold mb-2', colors.text.primary)}>
          Discovered Peers ({peers.length})
        </h3>
        {peers.length === 0 ? (
          <p className={cn('text-sm', colors.text.tertiary)}>
            No peers discovered yet. Ensure other devices are on the same network with sync enabled.
          </p>
        ) : (
          <div className="space-y-2">
            {peers.map((peer) => (
              <div
                key={peer.device_id}
                className={cn('flex items-center justify-between rounded-lg border p-3', colors.surface.elevated)}
              >
                <div>
                  <span className={cn('text-sm font-medium', colors.text.primary)}>
                    {peer.device_name}
                  </span>
                  <span className={cn('ml-2 text-xs font-mono', colors.text.tertiary)}>
                    {peer.device_id.slice(0, 8)}
                  </span>
                </div>
                <span className={cn('text-xs', colors.text.tertiary)}>
                  {peer.last_sync_at ? `Last: ${new Date(peer.last_sync_at).toLocaleString()}` : 'Never synced'}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
