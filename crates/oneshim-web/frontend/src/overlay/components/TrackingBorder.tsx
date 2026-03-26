interface Props {
  paused: boolean
  visible: boolean
}

export function TrackingBorder({ paused, visible }: Props) {
  if (!visible) return null

  const color = paused ? 'rgba(156,163,175,0.4)' : 'rgb(var(--brand-signal))'

  return (
    <div
      className="pointer-events-none fixed inset-0 z-[100]"
      style={{
        border: `3px solid ${color}`,
        boxShadow: paused ? undefined : `inset 0 0 10px ${color}`,
        animation: paused ? undefined : 'tracking-blink 2s ease-in-out infinite',
      }}
    />
  )
}
