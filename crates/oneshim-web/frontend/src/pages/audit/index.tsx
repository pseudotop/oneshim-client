import { useQuery } from '@tanstack/react-query'
import { ClipboardList } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { type AuditEntry, fetchAuditLogs, fetchAutomationStats } from '../../api/client'
import { EmptyState, ListSkeleton, Select, Skeleton, StatCardsSkeleton } from '../../components/ui'
import { Badge } from '../../components/ui/Badge'
import { Card, CardContent, CardHeader, CardTitle } from '../../components/ui/Card'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

function AuditLog() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [statusFilter, setStatusFilter] = useState<string>('')

  const { data: auditLogs, isLoading: logsLoading } = useQuery({
    queryKey: ['auditLogPage', statusFilter],
    queryFn: () => fetchAuditLogs(100, statusFilter || undefined),
    refetchInterval: 10_000,
  })

  const { data: stats, isLoading: statsLoading } = useQuery({
    queryKey: ['automationStats'],
    queryFn: fetchAutomationStats,
    refetchInterval: 10_000,
  })

  const statusBadge = (s: string) => {
    switch (s) {
      case 'Completed':
        return (
          <Badge color="success" size="sm">
            {t('automation.successful')}
          </Badge>
        )
      case 'Failed':
        return (
          <Badge color="error" size="sm">
            {t('automation.failed')}
          </Badge>
        )
      case 'Denied':
        return (
          <Badge color="warning" size="sm">
            {t('automation.denied')}
          </Badge>
        )
      case 'Timeout':
        return (
          <Badge color="purple" size="sm">
            {t('automation.timeout')}
          </Badge>
        )
      case 'Started':
        return (
          <Badge color="info" size="sm">
            {t('automation.started')}
          </Badge>
        )
      default:
        return (
          <Badge color="default" size="sm">
            {s}
          </Badge>
        )
    }
  }

  if (logsLoading || statsLoading) {
    return (
      <div className="min-h-full space-y-6 p-6">
        <Skeleton className="h-8 w-48" />
        <StatCardsSkeleton count={5} />
        <Skeleton className="h-10 w-full" />
        <ListSkeleton rows={8} />
      </div>
    )
  }

  if ((auditLogs?.length ?? 0) === 0 && (stats?.total_executions ?? 0) === 0) {
    return (
      <EmptyState
        icon={<ClipboardList className="h-8 w-8" />}
        title={t('emptyState.auditLog.title')}
        description={t('emptyState.auditLog.description')}
        action={{ label: t('emptyState.auditLog.action'), onClick: () => navigate('/automation') }}
      />
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('auditLog.title')}</h1>

      {/* Summary stats */}
      <div id="section-summary" className="grid grid-cols-2 gap-4 md:grid-cols-5">
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.totalExecutions')}</div>
            <div className={`mt-1 ${typography.stat.large} text-content`}>{stats?.total_executions ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.successful')}</div>
            <div className={`mt-1 ${typography.stat.large} text-semantic-success`}>{stats?.successful ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.failed')}</div>
            <div className={`mt-1 ${typography.stat.large} text-semantic-error`}>{stats?.failed ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.denied')}</div>
            <div className={`mt-1 ${typography.stat.large} text-semantic-warning`}>{stats?.denied ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.successRate')}</div>
            <div className={`mt-1 ${typography.stat.large} text-semantic-success`}>
              {((stats?.success_rate ?? 0) * 100).toFixed(1)}%
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Audit log table */}
      <Card id="section-entries">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{t('auditLog.entries')}</CardTitle>
            <Select
              value={statusFilter}
              selectSize="sm"
              onChange={(e) => setStatusFilter(e.target.value)}
              className="w-auto min-w-[9rem]"
            >
              <option value="">{t('common.all')}</option>
              <option value="Completed">{t('automation.successful')}</option>
              <option value="Failed">{t('automation.failed')}</option>
              <option value="Denied">{t('automation.denied')}</option>
              <option value="Timeout">{t('automation.timeout')}</option>
            </Select>
          </div>
        </CardHeader>
        <CardContent>
          {(auditLogs?.length ?? 0) === 0 ? (
            <p className="py-4 text-center text-content-secondary text-sm">{t('common.noData')}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-muted border-b">
                    <th className={`px-2 py-2 text-left ${typography.weight.medium} text-content-secondary`}>
                      {t('automation.time')}
                    </th>
                    <th className={`px-2 py-2 text-left ${typography.weight.medium} text-content-secondary`}>
                      {t('automation.actionType')}
                    </th>
                    <th className={`px-2 py-2 text-left ${typography.weight.medium} text-content-secondary`}>
                      {t('automation.statusLabel')}
                    </th>
                    <th className={`px-2 py-2 text-left ${typography.weight.medium} text-content-secondary`}>
                      {t('auditLog.details')}
                    </th>
                    <th className={`px-2 py-2 text-right ${typography.weight.medium} text-content-secondary`}>
                      {t('automation.elapsed')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {(auditLogs ?? []).map((entry: AuditEntry) => (
                    <tr key={entry.entry_id} className="border-muted border-b">
                      <td className="whitespace-nowrap px-2 py-2 text-content-strong">
                        {new Date(entry.timestamp).toLocaleString()}
                      </td>
                      <td className="px-2 py-2 text-content-strong">{entry.action_type}</td>
                      <td className="px-2 py-2">{statusBadge(entry.status)}</td>
                      <td className="max-w-xs truncate px-2 py-2 text-content-secondary" title={entry.details ?? ''}>
                        {entry.details ?? '-'}
                      </td>
                      <td className="px-2 py-2 text-right text-content-strong">
                        {entry.elapsed_ms != null ? `${entry.elapsed_ms}ms` : '-'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

export default AuditLog
