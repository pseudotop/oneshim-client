/**
 * InsightCard — displays AI-generated daily narrative and highlight chips.
 */
import { Card } from './ui'
import { cn } from '../utils/cn'
import { colors, typography } from '../styles/tokens'

interface DailyInsight {
  narrative: string
  highlights: Array<{
    highlight_type: string // ACHIEVEMENT, WARNING, SUGGESTION
    text: string
    segment_id?: string
  }>
}

interface InsightCardProps {
  insight: DailyInsight | null
}

const highlightConfig: Record<string, { icon: string; bg: string; text: string }> = {
  ACHIEVEMENT: { icon: '\u{1F3C6}', bg: 'bg-accent-green/10', text: 'text-accent-green' },
  WARNING: { icon: '\u{26A0}\u{FE0F}', bg: 'bg-semantic-warning/10', text: 'text-semantic-warning' },
  SUGGESTION: { icon: '\u{1F4A1}', bg: 'bg-accent-blue/10', text: 'text-accent-blue' },
}

export default function InsightCard({ insight }: InsightCardProps) {
  if (!insight) {
    return (
      <Card padding="md" className="border-l-4 border-l-blue-500">
        <p className={cn(typography.body, colors.text.secondary)}>No insight available for today</p>
      </Card>
    )
  }

  return (
    <Card padding="md" className="border-l-4 border-l-blue-500">
      <p className={cn('mb-3 leading-relaxed', typography.body, colors.text.primary)}>{insight.narrative}</p>
      {insight.highlights.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {insight.highlights.map((highlight, idx) => {
            const config = highlightConfig[highlight.highlight_type] ?? highlightConfig.SUGGESTION
            return (
              <span
                key={`${highlight.highlight_type}-${idx}`}
                className={cn('inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium', config.bg, config.text)}
              >
                <span aria-hidden="true">{config.icon}</span>
                {highlight.text}
              </span>
            )
          })}
        </div>
      )}
    </Card>
  )
}

InsightCard.displayName = 'InsightCard'
