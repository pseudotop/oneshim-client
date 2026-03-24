interface FocusModeIndicatorProps {
  active: boolean
}

export function FocusModeIndicator({ active }: FocusModeIndicatorProps) {
  if (!active) return null

  return (
    <div className="fixed top-3 left-1/2 -translate-x-1/2 z-50 pointer-events-none">
      <div className="px-3 py-1 rounded-full bg-brand/90 text-white text-[10px] font-semibold tracking-wider uppercase shadow-md backdrop-blur-sm">
        Focus Mode
      </div>
    </div>
  )
}
