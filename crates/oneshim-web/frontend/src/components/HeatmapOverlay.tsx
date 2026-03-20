/**
 * HeatmapOverlay — canvas-based GUI click-position heatmap.
 *
 * Renders colored circles at each 50x50 grid cell where user clicks
 * have been recorded. Intensity is proportional to click count.
 * Color scale: blue (low) -> yellow (medium) -> red (high).
 */
import { useEffect, useRef } from 'react'
import type { GuiHeatmapPoint } from '../api/contracts'

interface HeatmapOverlayProps {
  points: GuiHeatmapPoint[]
  maxCount: number
  /** Canvas width in CSS pixels; defaults to 1920 */
  width?: number
  /** Canvas height in CSS pixels; defaults to 1080 */
  height?: number
  className?: string
}

const BIN_SIZE = 50

/** Map a 0-1 ratio to an rgba color: blue -> yellow -> red. */
function heatColor(ratio: number): string {
  // Clamp
  const t = Math.max(0, Math.min(1, ratio))
  let r: number, g: number, b: number
  if (t < 0.5) {
    // blue (66,133,244) -> yellow (251,188,4)
    const s = t / 0.5
    r = Math.round(66 + (251 - 66) * s)
    g = Math.round(133 + (188 - 133) * s)
    b = Math.round(244 + (4 - 244) * s)
  } else {
    // yellow (251,188,4) -> red (234,67,53)
    const s = (t - 0.5) / 0.5
    r = Math.round(251 + (234 - 251) * s)
    g = Math.round(188 + (67 - 188) * s)
    b = Math.round(4 + (53 - 4) * s)
  }
  // Alpha scales from 0.25 at minimum to 0.85 at maximum
  const alpha = 0.25 + 0.6 * t
  return `rgba(${r},${g},${b},${alpha})`
}

export default function HeatmapOverlay({
  points,
  maxCount,
  width = 1920,
  height = 1080,
  className = '',
}: HeatmapOverlayProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    // Clear
    ctx.clearRect(0, 0, canvas.width, canvas.height)

    if (points.length === 0 || maxCount === 0) return

    const effectiveMax = maxCount || 1

    for (const pt of points) {
      const ratio = pt.count / effectiveMax
      const cx = pt.x + BIN_SIZE / 2
      const cy = pt.y + BIN_SIZE / 2
      // Radius scales with count — min 8px, max 24px
      const radius = 8 + 16 * ratio

      ctx.beginPath()
      ctx.arc(cx, cy, radius, 0, Math.PI * 2)
      ctx.fillStyle = heatColor(ratio)
      ctx.fill()
    }
  }, [points, maxCount, width, height])

  if (points.length === 0) {
    return (
      <div className={`flex items-center justify-center rounded-lg border border-dashed border-border p-8 ${className}`}>
        <p className="text-content-secondary text-sm">No GUI interaction data for this date.</p>
      </div>
    )
  }

  return (
    <div className={`relative overflow-hidden rounded-lg border border-border bg-surface ${className}`}>
      {/* Legend */}
      <div className="absolute top-2 right-2 z-10 flex items-center gap-1.5 rounded bg-surface/80 px-2 py-1 text-xs backdrop-blur-sm">
        <span className="text-content-secondary">Low</span>
        <div className="flex gap-0.5">
          <div className="h-2.5 w-2.5 rounded-full" style={{ background: heatColor(0) }} />
          <div className="h-2.5 w-2.5 rounded-full" style={{ background: heatColor(0.25) }} />
          <div className="h-2.5 w-2.5 rounded-full" style={{ background: heatColor(0.5) }} />
          <div className="h-2.5 w-2.5 rounded-full" style={{ background: heatColor(0.75) }} />
          <div className="h-2.5 w-2.5 rounded-full" style={{ background: heatColor(1) }} />
        </div>
        <span className="text-content-secondary">High</span>
      </div>

      <canvas
        ref={canvasRef}
        width={width}
        height={height}
        className="h-auto w-full"
        style={{ aspectRatio: `${width} / ${height}` }}
      />
    </div>
  )
}
