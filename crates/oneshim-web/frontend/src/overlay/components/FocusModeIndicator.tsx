interface FocusModeIndicatorProps {
  active: boolean
}

export function FocusModeIndicator({ active }: FocusModeIndicatorProps) {
  if (!active) return null

  return (
    <div className="pointer-events-none fixed top-3 left-1/2 z-50 -translate-x-1/2">
      <div className="rounded-full bg-brand/90 px-3 py-1 font-semibold text-[10px] text-white uppercase tracking-wider shadow-md backdrop-blur-sm">
        Focus Mode
      </div>
    </div>
  )
}
