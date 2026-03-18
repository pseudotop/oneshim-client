/**
 * StatisticsPanel — KPI cards, regime distribution bar, and longest focus highlight.
 */
import { Bar, BarChart, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { Card, CardTitle } from './ui'
import { cn } from '../utils/cn'
import { colors, typography } from '../styles/tokens'
import { useTheme } from '../contexts/ThemeContext'

interface DailyStatistics {
  deep_work_hours: number
  communication_hours: number
  meeting_hours: number
  context_switches: number
  longest_focus_mins: number
  longest_focus_content: string
  regime_distribution: Record<string, number> // label -> percentage
  comparison?: {
    deep_work_delta: number
    communication_delta: number
    context_switch_delta: number
  }
}

interface StatisticsPanelProps {
  statistics: DailyStatistics | null
}

// For deep work and focus, UP is good. For context switches and communication, DOWN is good.
function DeltaArrow({ delta, invertPositive = false }: { delta: number; invertPositive?: boolean }) {
  if (delta === 0) {
    return <span className={colors.text.tertiary}>-</span>
  }

  const isPositive = delta > 0
  // invertPositive: for context switches, fewer is better
  const isGood = invertPositive ? !isPositive : isPositive

  return (
    <span className={isGood ? 'text-accent-green' : 'text-accent-red'}>
      {isPositive ? '\u2191' : '\u2193'} {Math.abs(delta).toFixed(1)}h
    </span>
  )
}

function ContextSwitchDelta({ delta }: { delta: number }) {
  if (delta === 0) {
    return <span className={colors.text.tertiary}>-</span>
  }

  const isPositive = delta > 0
  // For context switches, fewer is better (DOWN = green)
  const isGood = !isPositive

  return (
    <span className={isGood ? 'text-accent-green' : 'text-accent-red'}>
      {isPositive ? '\u2191' : '\u2193'} {Math.abs(delta)}
    </span>
  )
}

// Deterministic palette for regime bars
const REGIME_COLORS = ['#14b8a6', '#3b82f6', '#8b5cf6', '#f59e0b', '#ef4444', '#10b981', '#6366f1', '#ec4899']

export default function StatisticsPanel({ statistics }: StatisticsPanelProps) {
  const { theme } = useTheme()

  if (!statistics) {
    return (
      <Card padding="md">
        <p className={cn(typography.body, colors.text.secondary, 'text-center py-8')}>
          No statistics available
        </p>
      </Card>
    )
  }

  const tooltipStyle = {
    backgroundColor: theme === 'dark' ? '#1e293b' : '#ffffff',
    border: theme === 'dark' ? 'none' : '1px solid #e2e8f0',
    borderRadius: '0.5rem',
    color: theme === 'dark' ? '#e2e8f0' : '#334155',
  }

  const regimeData = Object.entries(statistics.regime_distribution).map(([label, percentage], idx) => ({
    label,
    percentage,
    fill: REGIME_COLORS[idx % REGIME_COLORS.length],
  }))

  return (
    <div className="space-y-4">
      {/* KPI cards */}
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        <Card padding="md">
          <p className={cn('text-sm', colors.text.secondary)}>Deep Work</p>
          <p className={cn(typography.stat.large, colors.text.primary)}>
            {statistics.deep_work_hours.toFixed(1)}h
          </p>
          {statistics.comparison && (
            <DeltaArrow delta={statistics.comparison.deep_work_delta} />
          )}
        </Card>

        <Card padding="md">
          <p className={cn('text-sm', colors.text.secondary)}>Communication</p>
          <p className={cn(typography.stat.large, colors.text.primary)}>
            {statistics.communication_hours.toFixed(1)}h
          </p>
          {statistics.comparison && (
            <DeltaArrow delta={statistics.comparison.communication_delta} invertPositive />
          )}
        </Card>

        <Card padding="md">
          <p className={cn('text-sm', colors.text.secondary)}>Meetings</p>
          <p className={cn(typography.stat.large, colors.text.primary)}>
            {statistics.meeting_hours.toFixed(1)}h
          </p>
        </Card>

        <Card padding="md">
          <p className={cn('text-sm', colors.text.secondary)}>Context Switches</p>
          <p className={cn(typography.stat.large, colors.text.primary)}>
            {statistics.context_switches}
          </p>
          {statistics.comparison && (
            <ContextSwitchDelta delta={statistics.comparison.context_switch_delta} />
          )}
        </Card>
      </div>

      {/* Regime distribution bar */}
      {regimeData.length > 0 && (
        <Card padding="md">
          <CardTitle className="mb-4">Regime Distribution</CardTitle>
          <div className="h-48">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={regimeData} layout="vertical" barCategoryGap="20%">
                <XAxis type="number" domain={[0, 100]} tick={{ fill: '#94a3b8', fontSize: 12 }} unit="%" />
                <YAxis
                  type="category"
                  dataKey="label"
                  width={120}
                  tick={{ fill: '#94a3b8', fontSize: 12 }}
                />
                <Tooltip
                  contentStyle={tooltipStyle}
                  formatter={(value: number) => [`${value.toFixed(1)}%`, 'Share']}
                />
                <Bar dataKey="percentage" radius={[0, 4, 4, 0]}>
                  {regimeData.map((entry) => (
                    <Cell key={entry.label} fill={entry.fill} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </div>
          {/* Color legend */}
          <div className="mt-2 flex flex-wrap justify-center gap-4 text-xs">
            {regimeData.map((entry) => (
              <div key={entry.label} className="flex items-center gap-1.5">
                <div className="h-3 w-3 rounded-full" style={{ backgroundColor: entry.fill }} />
                <span className={colors.text.secondary}>{entry.label}</span>
              </div>
            ))}
          </div>
        </Card>
      )}

      {/* Longest focus highlight */}
      {statistics.longest_focus_mins > 0 && (
        <Card padding="md" className="border-l-4 border-l-emerald-500">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-accent-green/10 p-2">
              <span className="text-xl" aria-hidden="true">{'\u{1F3AF}'}</span>
            </div>
            <div>
              <p className={cn('font-semibold', colors.text.primary)}>
                Longest Focus: {statistics.longest_focus_mins} min
              </p>
              <p className={cn('text-sm', colors.text.secondary)}>
                {statistics.longest_focus_content}
              </p>
            </div>
          </div>
        </Card>
      )}
    </div>
  )
}

StatisticsPanel.displayName = 'StatisticsPanel'
