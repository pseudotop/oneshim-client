import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts'
import { HourlyMetrics } from '../api/client'
import { useTheme } from '../contexts/ThemeContext'
import { useTranslation } from 'react-i18next'

interface MetricsChartProps {
  data: HourlyMetrics[]
}

function formatHour(hourStr: string): string {
  try {
    const date = new Date(hourStr)
    return date.toLocaleTimeString('ko-KR', { hour: '2-digit', minute: '2-digit' })
  } catch {
    return hourStr
  }
}

export default function MetricsChart({ data }: MetricsChartProps) {
  const { t } = useTranslation()
  const { theme } = useTheme()
  const isDark = theme === 'dark'

  if (!data || data.length === 0) {
    return (
      <div className="h-64 flex items-center justify-center text-slate-400">
        {t('common.noData')}
      </div>
    )
  }

  const chartData = data.map((m) => ({
    hour: formatHour(m.hour),
    cpu: m.cpu_avg,
    cpuMax: m.cpu_max,
    memory: m.memory_avg / (1024 * 1024 * 1024), // GB로 변환
    memoryMax: m.memory_max / (1024 * 1024 * 1024),
  }))

  const axisStroke = isDark ? '#64748b' : '#94a3b8'
  const tickFill = isDark ? '#94a3b8' : '#64748b'

  return (
    <div className="h-64">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke={isDark ? '#334155' : '#e2e8f0'} />
          <XAxis
            dataKey="hour"
            stroke={axisStroke}
            tick={{ fill: tickFill, fontSize: 12 }}
          />
          <YAxis
            yAxisId="cpu"
            domain={[0, 100]}
            stroke={axisStroke}
            tick={{ fill: tickFill, fontSize: 12 }}
            tickFormatter={(v) => `${v}%`}
          />
          <YAxis
            yAxisId="memory"
            orientation="right"
            stroke={axisStroke}
            tick={{ fill: tickFill, fontSize: 12 }}
            tickFormatter={(v) => `${v.toFixed(0)}GB`}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: isDark ? '#1e293b' : '#ffffff',
              border: isDark ? '1px solid #334155' : '1px solid #e2e8f0',
              borderRadius: '8px',
            }}
            labelStyle={{ color: isDark ? '#f8fafc' : '#334155' }}
          />
          <Legend />
          <Line
            yAxisId="cpu"
            type="monotone"
            dataKey="cpu"
            name="CPU %"
            stroke="#14b8a6"
            strokeWidth={2}
            dot={false}
          />
          <Line
            yAxisId="memory"
            type="monotone"
            dataKey="memory"
            name="Memory (GB)"
            stroke="#3b82f6"
            strokeWidth={2}
            dot={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}
