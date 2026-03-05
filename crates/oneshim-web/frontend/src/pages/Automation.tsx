import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bot, CheckCircle2, ChevronDown, ChevronUp, Clock, XCircle } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import {
  type AuditEntry,
  deletePreset,
  fetchAuditLogs,
  fetchAutomationContracts,
  fetchAutomationStats,
  fetchAutomationStatus,
  fetchPolicies,
  fetchPresets,
  type PresetRunResult,
  runPreset,
  type WorkflowPreset,
} from '../api/client'
import { EmptyState, Select } from '../components/ui'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/Card'
import { Spinner } from '../components/ui/Spinner'
import { colors, interaction, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

type PresetTab = 'Productivity' | 'AppManagement' | 'Workflow' | 'Custom'

interface RunFeedback {
  presetId: string
  result: PresetRunResult
  timestamp: number
}

const sourceLabelByValue: Record<string, string> = {
  local: 'automation.sourceLocal',
  remote: 'automation.sourceRemote',
  'local-fallback': 'automation.sourceLocalFallback',
  'cli-subscription': 'automation.sourceCliSubscription',
  platform: 'automation.sourcePlatform',
}

const sourceBadgeColorByValue: Record<string, 'default' | 'info' | 'warning' | 'success' | 'primary'> = {
  local: 'default',
  remote: 'info',
  'local-fallback': 'warning',
  'cli-subscription': 'success',
  platform: 'primary',
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

  const { data: contracts } = useQuery({
    queryKey: ['automationContracts'],
    queryFn: fetchAutomationContracts,
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
    setRunFeedbacks((prev) => prev.filter((f) => f.presetId !== id))
    try {
      const result = await runPresetMutation.mutateAsync(id)
      setRunFeedbacks((prev) => [
        ...prev.filter((f) => f.presetId !== id),
        { presetId: id, result, timestamp: Date.now() },
      ])
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

  const getFeedback = (presetId: string): RunFeedback | undefined => runFeedbacks.find((f) => f.presetId === presetId)

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

  const filteredPresets = (presetsData?.presets ?? []).filter((p: WorkflowPreset) => p.category === presetTab)
  const ocrFallbackActive = status?.ocr_source === 'local-fallback' || Boolean(status?.ocr_fallback_reason)
  const llmFallbackActive = status?.llm_source === 'local-fallback' || Boolean(status?.llm_fallback_reason)
  const hasFallbackDetails = ocrFallbackActive || llmFallbackActive

  const sourceLabel = (source?: string | null) => t(sourceLabelByValue[source ?? ''] ?? 'automation.sourceUnknown')

  const sourceBadgeColor = (source?: string | null) => sourceBadgeColorByValue[source ?? ''] ?? 'default'

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
        icon={<Bot className="h-8 w-8" />}
        title={t('emptyState.automation.title')}
        description={t('emptyState.automation.description')}
        action={{ label: t('emptyState.automation.action'), onClick: () => navigate('/settings') }}
      />
    )
  }

  return (
    <div className="h-full space-y-6 overflow-y-auto p-6">
      <h1 className={cn(typography.h1, colors.text.primary)}>{t('automation.title')}</h1>

      {/* UI note */}
      <div className="grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-5">
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.status')}</div>
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
            <div className="text-content-secondary text-sm">{t('automation.sandbox')}</div>
            <div className="mt-1 font-semibold text-content text-lg">{status?.sandbox_profile ?? '-'}</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.ocrProvider')}</div>
            <div className="mt-1 font-semibold text-content text-lg">{status?.ocr_provider ?? '-'}</div>
            <div className="mt-2 flex items-center gap-2">
              <span className="text-content-secondary text-xs">{t('automation.providerSource')}</span>
              <Badge color={sourceBadgeColor(status?.ocr_source)} size="sm">
                {sourceLabel(status?.ocr_source)}
              </Badge>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.llmProvider')}</div>
            <div className="mt-1 font-semibold text-content text-lg">{status?.llm_provider ?? '-'}</div>
            <div className="mt-2 flex items-center gap-2">
              <span className="text-content-secondary text-xs">{t('automation.providerSource')}</span>
              <Badge color={sourceBadgeColor(status?.llm_source)} size="sm">
                {sourceLabel(status?.llm_source)}
              </Badge>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="text-content-secondary text-sm">{t('automation.pendingAudit')}</div>
            <div className="mt-1 font-semibold text-content text-lg">{status?.pending_audit_entries ?? 0}</div>
          </CardContent>
        </Card>
      </div>

      {hasFallbackDetails && (
        <Card>
          <CardHeader>
            <CardTitle>{t('automation.fallbackDetails')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3 text-sm">
              {ocrFallbackActive && (
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-content">{t('automation.ocrProvider')}</span>
                    <Badge color={sourceBadgeColor(status?.ocr_source)} size="sm">
                      {sourceLabel(status?.ocr_source)}
                    </Badge>
                  </div>
                  <div className="mt-1 text-content-secondary">
                    {t('automation.fallbackReason')}:{' '}
                    {status?.ocr_fallback_reason ?? t('automation.fallbackReasonUnavailable')}
                  </div>
                </div>
              )}
              {llmFallbackActive && (
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-content">{t('automation.llmProvider')}</span>
                    <Badge color={sourceBadgeColor(status?.llm_source)} size="sm">
                      {sourceLabel(status?.llm_source)}
                    </Badge>
                  </div>
                  <div className="mt-1 text-content-secondary">
                    {t('automation.fallbackReason')}:{' '}
                    {status?.llm_fallback_reason ?? t('automation.fallbackReasonUnavailable')}
                  </div>
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* UI note */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.presets')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-4 flex space-x-2">
            {(['Productivity', 'AppManagement', 'Workflow', 'Custom'] as PresetTab[]).map((tab) => (
              <button
                type="button"
                key={tab}
                onClick={() => setPresetTab(tab)}
                className={cn(
                  'rounded-md px-3 py-1.5 font-medium text-sm transition-colors',
                  interaction.focusRing,
                  presetTab === tab ? 'bg-accent-teal/10 text-accent-teal' : 'text-content-secondary hover:bg-hover',
                )}
              >
                {t(`automation.category.${tab}`)}
              </button>
            ))}
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
            {filteredPresets.map((preset: WorkflowPreset) => {
              const feedback = getFeedback(preset.id)
              const isExpanded = expandedPreset === preset.id
              return (
                <div
                  key={preset.id}
                  className={`flex flex-col rounded-lg border p-4 transition-colors ${
                    feedback
                      ? feedback.result.success
                        ? 'border-status-connected bg-semantic-success/10'
                        : 'border-status-error bg-semantic-error/10'
                      : 'border-muted'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div className="min-w-0 flex-1">
                      <h3 className="font-medium text-content">{preset.name}</h3>
                      <p className="mt-1 text-content-secondary text-sm">{preset.description}</p>
                    </div>
                    <div className="ml-2 flex shrink-0 items-center space-x-1">
                      {preset.platform && (
                        <Badge color="default" size="sm">
                          {preset.platform}
                        </Badge>
                      )}
                      {preset.builtin && (
                        <Badge color="info" size="sm">
                          {t('automation.builtin')}
                        </Badge>
                      )}
                    </div>
                  </div>

                  {/* UI note */}
                  <button
                    type="button"
                    onClick={() => setExpandedPreset(isExpanded ? null : preset.id)}
                    className={cn(
                      'mt-2 flex items-center text-content-muted text-xs transition-colors hover:text-content-strong',
                      interaction.focusRing,
                    )}
                  >
                    <span>
                      {preset.steps.length} {t('automation.steps')}
                    </span>
                    {preset.steps.length > 0 &&
                      (isExpanded ? <ChevronUp className="ml-1 h-3 w-3" /> : <ChevronDown className="ml-1 h-3 w-3" />)}
                  </button>

                  {/* UI note */}
                  {isExpanded && preset.steps.length > 0 && (
                    <div className="mt-2 space-y-1">
                      {preset.steps.map((step, idx) => (
                        <div key={step.name} className="flex items-center text-content-secondary text-xs">
                          <span className="mr-2 flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-hover font-medium text-[10px]">
                            {idx + 1}
                          </span>
                          <span className="truncate">{step.name}</span>
                          {step.delay_ms > 0 && (
                            <span className="ml-auto shrink-0 text-content-muted">+{step.delay_ms}ms</span>
                          )}
                        </div>
                      ))}
                    </div>
                  )}

                  {/* UI note */}
                  {feedback && (
                    <div
                      className={`mt-3 rounded-md p-2 text-xs ${
                        feedback.result.success
                          ? 'bg-semantic-success/20 text-semantic-success'
                          : 'bg-semantic-error/20 text-semantic-error'
                      }`}
                    >
                      <div className="flex items-center">
                        {feedback.result.success ? (
                          <CheckCircle2 className="mr-1.5 h-3.5 w-3.5 shrink-0" />
                        ) : (
                          <XCircle className="mr-1.5 h-3.5 w-3.5 shrink-0" />
                        )}
                        <span className="truncate font-medium">{feedback.result.message}</span>
                      </div>
                      {(feedback.result.steps_executed != null || feedback.result.total_elapsed_ms != null) && (
                        <div className="mt-1 ml-5 flex items-center space-x-3 text-[11px] opacity-80">
                          {feedback.result.steps_executed != null && feedback.result.total_steps != null && (
                            <span>
                              {feedback.result.steps_executed}/{feedback.result.total_steps} {t('automation.steps')}
                            </span>
                          )}
                          {feedback.result.total_elapsed_ms != null && (
                            <span className="flex items-center">
                              <Clock className="mr-0.5 h-3 w-3" />
                              {feedback.result.total_elapsed_ms}ms
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  )}

                  <div className="mt-auto flex items-center space-x-2 pt-3">
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
                      <Button variant="danger" size="sm" onClick={() => deletePresetMutation.mutate(preset.id)}>
                        {t('common.delete')}
                      </Button>
                    )}
                  </div>
                </div>
              )
            })}
            {filteredPresets.length === 0 && (
              <p className="col-span-full py-4 text-center text-content-secondary text-sm">
                {t('automation.noPresets')}
              </p>
            )}
          </div>
        </CardContent>
      </Card>

      {/* UI note */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.statsTitle')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4 md:grid-cols-5">
            <div className="text-center">
              <div className="font-bold text-2xl text-content">{stats?.total_executions ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.totalExecutions')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-accent-teal">{stats?.successful ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.successful')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-accent-red">{stats?.failed ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.failed')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-accent-orange">{stats?.denied ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.denied')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-semantic-warning">{stats?.timeout ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.timeout')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-accent-emerald">
                {((stats?.success_rate ?? 0) * 100).toFixed(1)}%
              </div>
              <div className="text-content-secondary text-xs">{t('automation.successRate')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-accent-orange">
                {((stats?.blocked_rate ?? 0) * 100).toFixed(1)}%
              </div>
              <div className="text-content-secondary text-xs">{t('automation.blockedRate')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-content">{(stats?.avg_elapsed_ms ?? 0).toFixed(0)}ms</div>
              <div className="text-content-secondary text-xs">{t('automation.avgElapsed')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-content">{(stats?.p95_elapsed_ms ?? 0).toFixed(0)}ms</div>
              <div className="text-content-secondary text-xs">{t('automation.p95Elapsed')}</div>
            </div>
            <div className="text-center">
              <div className="font-bold text-2xl text-content">{stats?.timing_samples ?? 0}</div>
              <div className="text-content-secondary text-xs">{t('automation.timingSamples')}</div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* UI note */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{t('automation.auditLog')}</CardTitle>
            <Select
              value={auditFilter}
              selectSize="sm"
              onChange={(e) => setAuditFilter(e.target.value)}
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
                    <th className="px-2 py-2 text-left font-medium text-content-secondary">{t('automation.time')}</th>
                    <th className="px-2 py-2 text-left font-medium text-content-secondary">
                      {t('automation.commandId')}
                    </th>
                    <th className="px-2 py-2 text-left font-medium text-content-secondary">
                      {t('automation.actionType')}
                    </th>
                    <th className="px-2 py-2 text-left font-medium text-content-secondary">
                      {t('automation.statusLabel')}
                    </th>
                    <th className="px-2 py-2 text-right font-medium text-content-secondary">
                      {t('automation.elapsed')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {(auditLogs ?? []).map((entry: AuditEntry) => (
                    <tr key={entry.entry_id} className="border-muted border-b">
                      <td className="whitespace-nowrap px-2 py-2 text-content-strong">
                        {new Date(entry.timestamp).toLocaleTimeString()}
                      </td>
                      <td className="px-2 py-2 font-mono text-content-strong text-xs">{entry.command_id}</td>
                      <td className="px-2 py-2 text-content-strong">{entry.action_type}</td>
                      <td className="px-2 py-2">{statusBadge(entry.status)}</td>
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

      {/* UI note */}
      <Card>
        <CardHeader>
          <CardTitle>{t('automation.policies')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 gap-4 text-sm md:grid-cols-2 lg:grid-cols-3">
            <div>
              <div className="text-content-secondary">{t('automation.automationEnabled')}</div>
              <div className="mt-1 font-medium text-content">
                {policies?.automation_enabled ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sandboxProfile')}</div>
              <div className="mt-1 font-medium text-content">{policies?.sandbox_profile ?? '-'}</div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sandboxEnabled')}</div>
              <div className="mt-1 font-medium text-content">
                {policies?.sandbox_enabled ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.allowNetwork')}</div>
              <div className="mt-1 font-medium text-content">
                {policies?.allow_network ? t('automation.enabled') : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.dataPolicy')}</div>
              <div className="mt-1 font-medium text-content">{policies?.external_data_policy ?? '-'}</div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sceneOverride')}</div>
              <div className="mt-1 font-medium text-content">
                {policies?.scene_action_override_active
                  ? t('automation.active')
                  : policies?.scene_action_override_enabled
                    ? t('automation.pending')
                    : t('automation.disabled')}
              </div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sceneOverrideExpires')}</div>
              <div className="mt-1 font-medium text-content">
                {policies?.scene_action_override_expires_at
                  ? new Date(policies.scene_action_override_expires_at).toLocaleString()
                  : '-'}
              </div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sceneOverrideIssue')}</div>
              <div className="mt-1 font-medium text-content">{policies?.scene_action_override_issue || '-'}</div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sceneSchemaVersion')}</div>
              <div className="mt-1 font-medium text-content">{contracts?.scene_schema_version ?? '-'}</div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.auditSchemaVersion')}</div>
              <div className="mt-1 font-medium text-content">{contracts?.audit_schema_version ?? '-'}</div>
            </div>
            <div>
              <div className="text-content-secondary">{t('automation.sceneActionSchemaVersion')}</div>
              <div className="mt-1 font-medium text-content">{contracts?.scene_action_schema_version ?? '-'}</div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

export default Automation
