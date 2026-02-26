import { BarChart, Bar, XAxis, YAxis, ResponsiveContainer, Tooltip, Cell } from 'recharts'
import { AppUsage } from '../api/client'
import { formatDuration } from '../utils/formatters'
import { useTranslation } from 'react-i18next'

interface AppUsageChartProps {
  apps: AppUsage[]
}

const COLORS = ['#14b8a6', '#3b82f6', '#8b5cf6', '#f59e0b', '#ef4444', '#10b981', '#6366f1', '#ec4899']

export default function AppUsageChart({ apps }: AppUsageChartProps) {
  const { t } = useTranslation()
  if (!apps || apps.length === 0) {
    return (
      <div className="h-64 flex items-center justify-center text-slate-400">
        {t('common.noData')}
      </div>
    )
  }

  const chartData = apps.slice(0, 8).map((app) => ({
    name: app.name.length > 15 ? app.name.slice(0, 15) + '...' : app.name,
    fullName: app.name,
    duration: app.duration_secs,
    durationStr: formatDuration(app.duration_secs),
    events: app.event_count,
  }))

  return (
    <div className="h-64">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={chartData} layout="vertical">
          <XAxis
            type="number"
            stroke="#64748b"
            tick={{ fill: '#94a3b8', fontSize: 12 }}
            tickFormatter={(v) => formatDuration(v)}
          />
          <YAxis
            type="category"
            dataKey="name"
            stroke="#64748b"
            tick={{ fill: '#94a3b8', fontSize: 12 }}
            width={100}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: '#1e293b',
              border: '1px solid #334155',
              borderRadius: '8px',
            }}
            labelStyle={{ color: '#f8fafc' }}
            formatter={(value: number, _name: string, props: any) => [
              formatDuration(value),
              props.payload.fullName,
            ]}
          />
          <Bar dataKey="duration" radius={[0, 4, 4, 0]}>
            {chartData.map((_, index) => (
              <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}
