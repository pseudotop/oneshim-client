import type { GoalProgressItem } from '../types'

interface GoalProgressBarProps {
  goals: GoalProgressItem[]
}

export default function GoalProgressBar({ goals }: GoalProgressBarProps) {
  return (
    <div className="fixed inset-x-0 bottom-0 z-40">
      {/* No per-element cursor passthrough — overlay interactivity is toggled
          globally via Cmd+Shift+O hotkey */}
      <div className="mx-auto max-w-3xl rounded-t-lg border border-b-0 border-white/10 bg-gray-900/80 px-4 py-2 backdrop-blur-md">
        <div className="flex flex-wrap gap-3">
          {goals.map((goal) => (
            <div key={goal.regime_label} className="flex min-w-[180px] flex-1 items-center gap-2">
              <span className="w-20 truncate text-xs text-gray-300">{goal.regime_label}</span>
              <div className="relative h-2 flex-1 overflow-hidden rounded-full bg-white/10">
                <div
                  className="absolute inset-y-0 left-0 rounded-full transition-all duration-500"
                  style={{
                    width: `${Math.min(goal.percentage, 100)}%`,
                    backgroundColor: goal.display_color,
                  }}
                />
              </div>
              <span className="w-16 text-right text-xs text-gray-400">
                {goal.current_minutes}/{goal.target_minutes}m
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
