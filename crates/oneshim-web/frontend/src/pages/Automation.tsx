import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { Bot, ChevronDown, ChevronUp, CheckCircle2, XCircle, Clock } from 'lucide-react'
import { Card, CardHeader, CardTitle, CardContent } from '../components/ui/Card'
import { Button } from '../components/ui/Button'
import { Badge } from '../components/ui/Badge'
import { Spinner } from '../components/ui/Spinner'
import { EmptyState } from '../components/ui'
import {
  fetchAutomationStatus,
  fetchAutomationStats,
  fetchAuditLogs,
  fetchPolicies,
  fetchPresets,
  runPreset,
  deletePreset,
  type AuditEntry,
  type WorkflowPreset,
  type PresetRunResult,
} from '../api/client'

type PresetTab = 'Productivity' | 'AppManagement' | 'Workflow' | 'Custom'

/** 프리셋 실행 결과 (UI 표시용) */
interface RunFeedback {
  presetId: string
  result: PresetRunResult
  timestamp: number
}

function Automation() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [auditFilter, setAuditFilter] = useState<string>('')
  const [presetTab, setPresetTab] = useState<PresetTab>('Productivity')
  const [runningPreset, setRunningPreset] = useState<string | null>(null)
  const [runFeedbacks, setRunFeedbacks] = useState<RunFeedback[]>([])
  const [expandedPreset, setExpandedPreset] = useState<string | null>(null)

  const { data: status, isLoading: statusLoading } = useQuery({
    queryKey: ['automationStatus'],
    queryFn: fetchAutomationStatus,
    refetchInterval: 30000,
  })

  const { data: stats } = useQuery({
    queryKey: ['automationStats'],
    queryFn: fetchAutomationStats,
    refetchInterval: 30000,
  })

  const { data: auditLogs } = useQuery({
    queryKey: ['auditLogs', auditFilter],
    queryFn: () => fetchAuditLogs(50, auditFilter || undefined),
    refetchInterval: 30000,
  })

  const { data: policies } = useQuery({
    queryKey: ['policies'],
    queryFn: fetchPolicies,
  })

  const { data: presetsData } = useQuery({
    queryKey: ['presets'],
    queryFn: fetchPresets,
  })

  const runPresetMutation = useMutation({
    mutationFn: runPreset,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auditLogs'] })
      queryClient.invalidateQueries({ queryKey: ['automationStats'] })
    },
  })

  const deletePresetMutation = useMutation({
    mutationFn: deletePreset,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['presets'] })
    },
  })

  const handleRunPreset = async (id: string) => {
    setRunningPreset(id)
    // 이전 피드백 제거
    setRunFeedbacks((prev) => prev.filter((f) => f.presetId !== id))
    try {
      const result = await runPresetMutation.mutateAsync(id)
      setRunFeedbacks((prev) => [
        ...prev.filter((f) => f.presetId !== id),
        { presetId: id, result, timestamp: Date.now() },
      ])
      // 5초 후 피드백 자동 제거
      setTimeout(() => {
        setRunFeedbacks((prev) => prev.filter((f) => f.presetId !== id))
      }, 8000)
    } catch (error) {
      const message = error instanceof Error ? error.message : t('automation.runError')
      setRunFeedbacks((prev) => [
        ...prev.filter((f) => f.presetId !== id),
        {
          presetId: id,
          result: { preset_id: id, success: false, message },
          timestamp: Date.now(),
        },
      ])
    } finally {
      setRunningPreset(null)
    }
  }

  const getFeedback = (presetId: string): RunFeedback | undefined =>
    runFeedbacks.find((f) => f.presetId === presetId)

  const statusBadge = (s: string) => {
    switch (s) {
      case 'Completed': return <Badge color="success" size="sm">{t('automation.successful')}</Badge>
      case 'Failed': return <Badge color="error" size="sm">{t('automation.failed')}</Badge>
      case 'Denied': return <Badge color="warning" size="sm">{t('automation.denied')}</Badge>
      case 'Timeout': return <Badge color="purple" size="sm">{t('automation.timeout')}</Badge>
      case 'Started': return <Badge color="info" size="sm">{t('automation.started')}</Badge>
      default: return <Badge color="default" size="sm">{s}</Badge>
    }
  }

  const filteredPresets = (presetsData?.presets ?? []).filter((p: WorkflowPreset) => p.category === presetTab)

  if (statusLoading) {
    return (
      <div className="flex justify-center py-12">
        <Spinner size="lg" />
      </div>
    )
  }

  if ((auditLogs?.length ?? 0) === 0 && (stats?.total_executions ?? 0) === 0 && !status?.enabled) {
    return (
      <EmptyState
        icon={<Bot className="w-8 h-8" />}
        title={t('emptyState.automation.title')}
        description={t('emptyState.automation.description')}
        action={{ label: t('emptyState.automation.action'), onClick: () => navigate('/settings') }}
      />
    )
  }

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
        {t('automation.title')}
      </h1>

      {/* 상태 카드 */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent>
            <div className="text-sm text-slate-500 dark:text-slate-400">{t('automation.status')}</div>
            <div className="mt-1">
              {status?.enabled ? (
                <Badge color="success">{t('automation.enabled')}</Badge>
              ) : (
                <Badge color="error">{t('automation.disabled')}</Badge>
              )}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-sm text-slate-500 dark:text-slate-400">{t('automation.sandbox')}</div>
            <div className="mt-1 text-lg font-semibold text-slate-900 dark:text-white">
              {status?.sandbox_profile ?? '-'}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-sm text-slate-500 dark:text-slate-400">{t('automation.ocrProvider')}</div>
            <div className="mt-1 text-lg font-semibold text-slate-900 dark:text-white">
              {status?.ocr_provider ?? '-'}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-sm text-slate-500 dark:text-slate-400">{t('automation.pendingAudit')}</div>
            <div className="mt-1 text-lg font-semibold text-slate-900 dark:text-white">
              {status?.pending_audit_entries ?? 0}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* 워크플로우 프리셋 */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.presets')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex space-x-2 mb-4">
            {(['Productivity', 'AppManagement', 'Workflow', 'Custom'] as PresetTab[]).map((tab) => (
              <button
                key={tab}
                onClick={() => setPresetTab(tab)}
                className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                  presetTab === tab
                    ? 'bg-teal-100 dark:bg-teal-900/30 text-teal-700 dark:text-teal-300'
                    : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'
                }`}
              >
                {t(`automation.category.${tab}`)}
              </button>
            ))}
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            {filteredPresets.map((preset: WorkflowPreset) => {
              const feedback = getFeedback(preset.id)
              const isExpanded = expandedPreset === preset.id
              return (
                <div
                  key={preset.id}
                  className={`border rounded-lg p-4 flex flex-col transition-colors ${
                    feedback
                      ? feedback.result.success
                        ? 'border-green-300 dark:border-green-700 bg-green-50/50 dark:bg-green-900/10'
                        : 'border-red-300 dark:border-red-700 bg-red-50/50 dark:bg-red-900/10'
                      : 'border-slate-200 dark:border-slate-700'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium text-slate-900 dark:text-white">{preset.name}</h3>
                      <p className="text-sm text-slate-500 dark:text-slate-400 mt-1">{preset.description}</p>
                    </div>
                    <div className="flex items-center space-x-1 ml-2 shrink-0">
                      {preset.platform && (
                        <Badge color="default" size="sm">{preset.platform}</Badge>
                      )}
                      {preset.builtin && <Badge color="info" size="sm">{t('automation.builtin')}</Badge>}
                    </div>
                  </div>

                  {/* 단계 수 + 확장 토글 */}
                  <button
                    onClick={() => setExpandedPreset(isExpanded ? null : preset.id)}
                    className="mt-2 flex items-center text-xs text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 transition-colors"
                  >
                    <span>{preset.steps.length} {t('automation.steps')}</span>
                    {preset.steps.length > 0 && (
                      isExpanded
                        ? <ChevronUp className="w-3 h-3 ml-1" />
                        : <ChevronDown className="w-3 h-3 ml-1" />
                    )}
                  </button>

                  {/* 단계 상세 (확장 시) */}
                  {isExpanded && preset.steps.length > 0 && (
                    <div className="mt-2 space-y-1">
                      {preset.steps.map((step, idx) => (
                        <div
                          key={idx}
                          className="flex items-center text-xs text-slate-500 dark:text-slate-400"
                        >
                          <span className="w-4 h-4 flex items-center justify-center rounded-full bg-slate-200 dark:bg-slate-700 text-[10px] font-medium mr-2 shrink-0">
                            {idx + 1}
                          </span>
                          <span className="truncate">{step.name}</span>
                          {step.delay_ms > 0 && (
                            <span className="ml-auto text-slate-400 dark:text-slate-600 shrink-0">
                              +{step.delay_ms}ms
                            </span>
                          )}
                        </div>
                      ))}
                    </div>
                  )}

                  {/* 실행 결과 피드백 */}
                  {feedback && (
                    <div className={`mt-3 p-2 rounded-md text-xs ${
                      feedback.result.success
                        ? 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300'
                        : 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300'
                    }`}>
                      <div className="flex items-center">
                        {feedback.result.success ? (
                          <CheckCircle2 className="w-3.5 h-3.5 mr-1.5 shrink-0" />
                        ) : (
                          <XCircle className="w-3.5 h-3.5 mr-1.5 shrink-0" />
                        )}
                        <span className="font-medium truncate">{feedback.result.message}</span>
                      </div>
                      {(feedback.result.steps_executed != null || feedback.result.total_elapsed_ms != null) && (
                        <div className="flex items-center mt-1 ml-5 space-x-3 text-[11px] opacity-80">
                          {feedback.result.steps_executed != null && feedback.result.total_steps != null && (
                            <span>{feedback.result.steps_executed}/{feedback.result.total_steps} {t('automation.steps')}</span>
                          )}
                          {feedback.result.total_elapsed_ms != null && (
                            <span className="flex items-center">
                              <Clock className="w-3 h-3 mr-0.5" />
                              {feedback.result.total_elapsed_ms}ms
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  )}

                  <div className="mt-auto pt-3 flex items-center space-x-2">
                    <Button
                      variant="primary"
                      size="sm"
                      isLoading={runningPreset === preset.id}
                      onClick={() => handleRunPreset(preset.id)}
                      disabled={!status?.enabled}
                    >
                      {t('automation.run')}
                    </Button>
                    {!preset.builtin && (
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => deletePresetMutation.mutate(preset.id)}
                      >
                        {t('common.delete')}
                      </Button>
                    )}
                  </div>
                </div>
              )
            })}
            {filteredPresets.length === 0 && (
              <p className="text-sm text-slate-500 dark:text-slate-400 col-span-full py-4 text-center">
                {t('automation.noPresets')}
              </p>
            )}
          </div>
        </CardContent>
      </Card>

      {/* 실행 통계 */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.statsTitle')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-6 gap-4">
            <div className="text-center">
              <div className="text-2xl font-bold text-slate-900 dark:text-white">{stats?.total_executions ?? 0}</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.totalExecutions')}</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-teal-600 dark:text-teal-400">{stats?.successful ?? 0}</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.successful')}</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-red-600 dark:text-red-400">{stats?.failed ?? 0}</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.failed')}</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">{stats?.denied ?? 0}</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.denied')}</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-yellow-600 dark:text-yellow-400">{stats?.timeout ?? 0}</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.timeout')}</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-slate-900 dark:text-white">{(stats?.avg_elapsed_ms ?? 0).toFixed(0)}ms</div>
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('automation.avgElapsed')}</div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 감사 로그 */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{t('automation.auditLog')}</CardTitle>
            <select
              value={auditFilter}
              onChange={(e) => setAuditFilter(e.target.value)}
              className="px-2 py-1 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white"
            >
              <option value="">{t('common.all')}</option>
              <option value="Completed">{t('automation.successful')}</option>
              <option value="Failed">{t('automation.failed')}</option>
              <option value="Denied">{t('automation.denied')}</option>
              <option value="Timeout">{t('automation.timeout')}</option>
            </select>
          </div>
        </CardHeader>
        <CardContent>
          {(auditLogs?.length ?? 0) === 0 ? (
            <p className="text-sm text-slate-500 dark:text-slate-400 text-center py-4">{t('common.noData')}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-slate-200 dark:border-slate-700">
                    <th className="text-left py-2 px-2 text-slate-500 dark:text-slate-400 font-medium">{t('automation.time')}</th>
                    <th className="text-left py-2 px-2 text-slate-500 dark:text-slate-400 font-medium">{t('automation.commandId')}</th>
                    <th className="text-left py-2 px-2 text-slate-500 dark:text-slate-400 font-medium">{t('automation.actionType')}</th>
                    <th className="text-left py-2 px-2 text-slate-500 dark:text-slate-400 font-medium">{t('automation.statusLabel')}</th>
                    <th className="text-right py-2 px-2 text-slate-500 dark:text-slate-400 font-medium">{t('automation.elapsed')}</th>
                  </tr>
                </thead>
                <tbody>
                  {(auditLogs ?? []).map((entry: AuditEntry) => (
                    <tr key={entry.entry_id} className="border-b border-slate-100 dark:border-slate-800">
                      <td className="py-2 px-2 text-slate-700 dark:text-slate-300 whitespace-nowrap">
                        {new Date(entry.timestamp).toLocaleTimeString()}
                      </td>
                      <td className="py-2 px-2 text-slate-700 dark:text-slate-300 font-mono text-xs">
                        {entry.command_id}
                      </td>
                      <td className="py-2 px-2 text-slate-700 dark:text-slate-300">{entry.action_type}</td>
                      <td className="py-2 px-2">{statusBadge(entry.status)}</td>
                      <td className="py-2 px-2 text-right text-slate-700 dark:text-slate-300">
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

      {/* 정책 정보 */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.policies')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-5 gap-4 text-sm">
            <div>
              <div className="text-slate-500 dark:text-slate-400">{t('automation.automationEnabled')}</div>
              <div className="mt-1 font-medium text-slate-900 dark:text-white">
                {policies?.automation_enabled ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-slate-500 dark:text-slate-400">{t('automation.sandboxProfile')}</div>
              <div className="mt-1 font-medium text-slate-900 dark:text-white">{policies?.sandbox_profile ?? '-'}</div>
            </div>
            <div>
              <div className="text-slate-500 dark:text-slate-400">{t('automation.sandboxEnabled')}</div>
              <div className="mt-1 font-medium text-slate-900 dark:text-white">
                {policies?.sandbox_enabled ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-slate-500 dark:text-slate-400">{t('automation.allowNetwork')}</div>
              <div className="mt-1 font-medium text-slate-900 dark:text-white">
                {policies?.allow_network ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-slate-500 dark:text-slate-400">{t('automation.dataPolicy')}</div>
              <div className="mt-1 font-medium text-slate-900 dark:text-white">{policies?.external_data_policy ?? '-'}</div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

export default Automation
