import { ProcessSnapshot } from '../api/client'
import { formatBytes } from '../utils/formatters'

interface ProcessListProps {
  snapshot: ProcessSnapshot
}

export default function ProcessList({ snapshot }: ProcessListProps) {
  const processes = snapshot.processes.slice(0, 10)

  if (processes.length === 0) {
    return (
      <div className="text-center py-8 text-slate-400">
        프로세스 데이터 없음
      </div>
    )
  }

  return (
    <div className="space-y-2">
      {processes.map((proc, index) => (
        <div
          key={`${proc.pid}-${index}`}
          className="flex items-center justify-between p-3 bg-slate-100 dark:bg-slate-900 rounded-lg"
        >
          <div className="flex items-center space-x-3">
            <span className="text-slate-500 text-sm w-6">{index + 1}</span>
            <div>
              <div className="font-medium text-slate-900 dark:text-white">{proc.name}</div>
              <div className="text-xs text-slate-500">PID: {proc.pid}</div>
            </div>
          </div>
          <div className="flex items-center space-x-4 text-right">
            <div>
              <div className="text-sm font-medium text-teal-400">
                {proc.cpu_usage.toFixed(1)}%
              </div>
              <div className="text-xs text-slate-500">CPU</div>
            </div>
            <div>
              <div className="text-sm font-medium text-blue-400">
                {formatBytes(proc.memory_bytes)}
              </div>
              <div className="text-xs text-slate-500">Memory</div>
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}
