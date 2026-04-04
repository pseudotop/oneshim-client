import { cn } from '../../utils/cn'

interface SnoozeOption {
  label: string
  minutes: number
}

const options: SnoozeOption[] = [
  { label: '30 minutes', minutes: 30 },
  { label: '1 hour', minutes: 60 },
  { label: '2 hours', minutes: 120 },
  { label: '4 hours', minutes: 240 },
  { label: 'Tomorrow 9 AM', minutes: 0 }, // calculated at runtime
]

interface SnoozePopoverProps {
  onSelect: (minutes: number) => void
  onCancel: () => void
}

function minutesToTomorrow9AM(): number {
  const now = new Date()
  const tomorrow = new Date(now)
  tomorrow.setDate(tomorrow.getDate() + 1)
  tomorrow.setHours(9, 0, 0, 0)
  return Math.ceil((tomorrow.getTime() - now.getTime()) / 60000)
}

export function SnoozePopover({ onSelect, onCancel }: SnoozePopoverProps) {
  return (
    <div className={cn(
      'absolute bottom-full right-0 mb-1 z-10',
      'bg-surface-sunken/95 backdrop-blur-md rounded-lg shadow-lg',
      'border border-border-default p-1 min-w-[140px]',
    )}>
      {options.map(opt => (
        <button
          key={opt.label}
          type="button"
          className="w-full text-left px-3 py-1.5 text-xs text-content-primary rounded hover:bg-content-inverse/10 transition-colors"
          onClick={() => onSelect(opt.minutes === 0 ? minutesToTomorrow9AM() : opt.minutes)}
        >
          {opt.label}
        </button>
      ))}
      <button
        type="button"
        className="w-full text-left px-3 py-1.5 text-xs text-content-secondary rounded hover:bg-content-inverse/10 transition-colors mt-0.5 border-t border-border-default"
        onClick={onCancel}
      >
        Cancel
      </button>
    </div>
  )
}
