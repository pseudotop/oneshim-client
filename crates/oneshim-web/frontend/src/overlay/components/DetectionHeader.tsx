import { memo } from 'react'

interface DetectionHeaderProps {
  elementCount: number
  onRefresh: () => void
  onClose: () => void
}

export default memo(function DetectionHeader({ elementCount, onRefresh, onClose }: DetectionHeaderProps) {
  const isMac = navigator.platform.startsWith('Mac')
  const refreshKey = isMac ? '\u2318\u21e7R' : 'Ctrl+Shift+R'
  const closeKey = isMac ? '\u2318\u21e7D' : 'Ctrl+Shift+D'

  return (
    <div
      className="fixed top-0 right-0 left-0 z-detection-header flex items-center justify-between px-4 text-[11px] text-white backdrop-blur-md"
      style={{ height: 28, backgroundColor: 'rgb(0 0 0 / 0.75)' }}
    >
      <div className="flex items-center gap-3">
        <span className="font-medium">Detection Mode</span>
        <span className="text-white/50">{elementCount} elements</span>
      </div>
      <div className="flex items-center gap-3">
        <button
          type="button"
          className="rounded px-1.5 py-0.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
          onClick={onRefresh}
          title={`Refresh (${refreshKey})`}
          aria-label={`Refresh detection (${refreshKey})`}
        >
          Refresh {refreshKey}
        </button>
        <button
          type="button"
          className="rounded px-1.5 py-0.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
          onClick={onClose}
          title={`Close (${closeKey})`}
          aria-label={`Close detection overlay (${closeKey})`}
        >
          Close
        </button>
      </div>
    </div>
  )
})
