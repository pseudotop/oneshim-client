import { useQuery } from '@tanstack/react-query'
import { ClipboardList } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Outlet, useNavigate } from 'react-router-dom'
import { type AuditEntry, fetchAuditLogs, fetchAutomationStats } from '../../api/client'
import type { AutomationStats } from '../../api/contracts'
import { EmptyState, ListSkeleton, Skeleton, StatCardsSkeleton } from '../../components/ui'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export interface AuditOutletContext {
  auditLogs: AuditEntry[] | undefined
  logsLoading: boolean
  stats: AutomationStats | undefined
  statsLoading: boolean
}

export default function AuditLayout() {
  const { t } = useTranslation()
  const navigate = useNavigate()

  const { data: auditLogs, isLoading: logsLoading } = useQuery({
    queryKey: ['auditLogPage', ''],
    queryFn: () => fetchAuditLogs(100, undefined),
    refetchInterval: 10_000,
  })

  const { data: stats, isLoading: statsLoading } = useQuery({
    queryKey: ['automationStats'],
    queryFn: fetchAutomationStats,
    refetchInterval: 10_000,
  })

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

  const ctx: AuditOutletContext = { auditLogs, logsLoading, stats, statsLoading }

  return (
    <div className="min-h-full space-y-6 p-6">
      <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('auditLog.title')}</h1>
      <Outlet context={ctx} />
    </div>
  )
}
