import { useTranslation } from 'react-i18next'
import { Bar, BarChart, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { AppUsage } from '../api/client'
import { chartPalette, palette } from '../styles/tokens'
import { formatDuration } from '../utils/formatters'

interface AppUsageChartProps {
  apps: AppUsage[]
}

export default function AppUsageChart({ apps }: AppUsageChartProps) {
  const { t } = useTranslation()
  if (!apps || apps.length === 0) {
    return <div className="flex h-64 items-center justify-center text-content-muted">{t('common.noData')}</div>
  }

  const chartData = apps.slice(0, 8).map((app) => ({
    name: app.name.length > 15 ? `${app.name.slice(0, 15)}...` : app.name,
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
            stroke={palette.gray500}
            tick={{ fill: palette.gray500, fontSize: 12 }}
            tickFormatter={(v) => formatDuration(v)}
          />
          <YAxis
            type="category"
            dataKey="name"
            stroke={palette.gray500}
            tick={{ fill: palette.gray500, fontSize: 12 }}
            width={100}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: '#1e293b',
              border: '1px solid #334155',
              borderRadius: '8px',
            }}
            labelStyle={{ color: '#f8fafc' }}
            formatter={(value: number, _name: string, props: { payload?: { fullName: string } }) => [
              formatDuration(value),
              props.payload?.fullName ?? '',
            ]}
          />
          <Bar dataKey="duration" radius={[0, 4, 4, 0]}>
            {chartData.map((entry, index) => (
              <Cell key={entry.fullName} fill={chartPalette[index % chartPalette.length]} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}
