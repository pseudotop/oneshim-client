import type { FocusHighlightPayload } from '../types'

interface FocusHighlightProps {
  highlight: FocusHighlightPayload
}

export default function FocusHighlight({ highlight }: FocusHighlightProps) {
  const { x, y, width, height, border_color, opacity } = highlight

  return (
    <div
      className="pointer-events-none fixed transition-all duration-200 ease-out"
      style={{
        left: x,
        top: y,
        width,
        height,
        border: `2px solid ${border_color}`,
        borderRadius: '4px',
        opacity,
        boxShadow: `0 0 12px ${border_color}40`,
      }}
    />
  )
}
