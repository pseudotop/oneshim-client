import { useEffect, useState } from 'react'
import { motion, typography } from '../../styles/tokens'

interface FocusModeIndicatorProps {
  active: boolean
}

export default function FocusModeIndicator({ active }: FocusModeIndicatorProps) {
  const [visible, setVisible] = useState(false)

  // Fade in when active becomes true; fade out then unmount when false.
  useEffect(() => {
    if (active) {
      // Small delay so the browser paints opacity-0 first, then transitions to opacity-100
      const id = requestAnimationFrame(() => setVisible(true))
      return () => cancelAnimationFrame(id)
    }
    setVisible(false)
  }, [active])

  // Don't render at all once the fade-out completes
  if (!active && !visible) return null

  return (
    <div
      className={`fixed left-4 top-4 z-50 pointer-events-none ${motion.opacity} ${motion.duration.normal} ${
        visible ? 'opacity-100' : 'opacity-0'
      }`}
    >
      <div className="flex items-center gap-1.5 rounded-full border border-content-inverse/10 bg-surface-sunken/80 px-3 py-1 shadow-lg backdrop-blur-md">
        {/* Pulsing dot */}
        <span className="relative flex h-2 w-2">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-brand-signal opacity-75" />
          <span className="relative inline-flex h-2 w-2 rounded-full bg-brand-signal" />
        </span>
        <span className={`${typography.caption} ${typography.weight.medium} text-content-secondary select-none`}>
          Focus
        </span>
      </div>
    </div>
  )
}
