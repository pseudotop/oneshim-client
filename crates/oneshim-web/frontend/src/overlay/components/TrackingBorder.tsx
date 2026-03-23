interface Props {
  paused: boolean
  visible: boolean
}

export function TrackingBorder({ paused, visible }: Props) {
  if (!visible) return null

  return (
    <div
      className={`fixed inset-0 pointer-events-none z-[100] border-[3px] ${
        paused
          ? 'border-gray-400/40'
          : 'border-brand-signal/60 animate-tracking-pulse'
      }`}
    />
  )
}
