interface Props {
  paused: boolean
  visible: boolean
}

export function TrackingBorder({ paused, visible }: Props) {
  if (!visible) return null

  return (
    <div
      className={`pointer-events-none fixed inset-0 z-[100] border-[3px] ${
        paused ? 'border-gray-400/40' : 'animate-tracking-blink border-brand-signal'
      }`}
      style={
        paused
          ? undefined
          : { boxShadow: 'inset 0 0 10px rgb(var(--brand-signal))' }
      }
    />
  )
}
