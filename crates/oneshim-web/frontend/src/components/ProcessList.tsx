import type { ProcessSnapshot } from '../api/client'
import { formatBytes } from '../utils/formatters'

interface ProcessListProps {
  snapshot: ProcessSnapshot
}

export default function ProcessList({ snapshot }: ProcessListProps) {
  const processes = snapshot.processes.slice(0, 10)

  if (processes.length === 0) {
    return <div className="py-8 text-center text-content-muted">프로세스 데이터 none</div>
  }

  return (
    <div className="space-y-2">
      {processes.map((proc, index) => (
        <div key={proc.pid} className="flex items-center justify-between rounded-lg bg-surface-muted p-3">
          <div className="flex items-center space-x-3">
            <span className="w-6 text-content-tertiary text-sm">{index + 1}</span>
            <div>
              <div className="font-medium text-content">{proc.name}</div>
              <div className="text-content-tertiary text-xs">PID: {proc.pid}</div>
            </div>
          </div>
          <div className="flex items-center space-x-4 text-right">
            <div>
              <div className="font-medium text-sm text-teal-400">{proc.cpu_usage.toFixed(1)}%</div>
              <div className="text-content-tertiary text-xs">CPU</div>
            </div>
            <div>
              <div className="font-medium text-blue-400 text-sm">{formatBytes(proc.memory_bytes)}</div>
              <div className="text-content-tertiary text-xs">Memory</div>
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}
