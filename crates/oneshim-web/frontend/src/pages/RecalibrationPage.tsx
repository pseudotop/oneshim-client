/**
 * RecalibrationPage — bulk regime correction with date-range filtering,
 * segment list, override history, and recluster trigger.
 */
import { useQuery } from '@tanstack/react-query'
import { RefreshCw, Trash2 } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchDailyDigest } from '../api/client'
import type { CreateOverrideRequest, DailyDigestResponse, DailyDigestSegment, RegimeOverride } from '../api/contracts'
import DateRangePicker from '../components/DateRangePicker'
import { Badge, Button, Card, EmptyState, Select, Skeleton, Spinner } from '../components/ui'
import { useCreateOverride, useDeleteOverride, useOverrides, useRecluster } from '../hooks/useRecalibration'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

// Default regime options
const REGIME_OPTIONS = [
  { id: 'deep-work', label: 'Deep Work' },
  { id: 'communication', label: 'Communication' },
  { id: 'meeting', label: 'Meeting' },
  { id: 'break', label: 'Break' },
  { id: 'admin', label: 'Admin' },
]


function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  } catch {
    return iso.slice(11, 16)
  }
}

function formatDateTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    })
  } catch {
    return iso
  }
}

function getActionLabel(override: RegimeOverride, t: (key: string) => string): string {
  switch (override.user_action.type) {
    case 'MARK_AS_PERSONAL_TIME':
      return t('recalibration.personalTime')
    case 'MARK_AS_NOISE':
      return t('recalibration.personalTime')
    case 'REASSIGN_REGIME': {
      const action = override.user_action as { type: 'REASSIGN_REGIME'; target_regime_id: string }
      const regime = REGIME_OPTIONS.find((r) => r.id === action.target_regime_id)
      return regime?.label ?? action.target_regime_id
    }
    default:
      return t('recalibration.overridden')
  }
}

export default function RecalibrationPage() {
  const { t } = useTranslation()
  const [from, setFrom] = useState<string | undefined>()
  const [to, setTo] = useState<string | undefined>()

  const handleRangeChange = useCallback((newFrom: string | undefined, newTo: string | undefined) => {
    setFrom(newFrom)
    setTo(newTo)
  }, [])

  // Fetch overrides
  const { data: overrides, isLoading: overridesLoading } = useOverrides(from, to)

  // Fetch segments from daily digest for the date range
  const { data: digestData, isLoading: segmentsLoading } = useQuery<DailyDigestResponse>({
    queryKey: ['recalibration-segments', from, to],
    queryFn: async () => {
      // Use the date part only for the API call
      const dateStr = from ? from.split('T')[0] : new Date().toISOString().split('T')[0]
      return fetchDailyDigest(dateStr)
    },
    enabled: !!from,
  })

  // Mutations
  const createOverride = useCreateOverride()
  const deleteOverride = useDeleteOverride()
  const recluster = useRecluster()

  // Build override lookup
  const overrideMap = useMemo(() => {
    const map = new Map<string, RegimeOverride>()
    if (overrides) {
      for (const o of overrides) {
        map.set(o.segment_id, o)
      }
    }
    return map
  }, [overrides])

  const segments = digestData?.timeline ?? []

  const handleMarkAsPersonal = (seg: DailyDigestSegment) => {
    const req: CreateOverrideRequest = {
      segment_id: seg.segment_id,
      original_regime_id: seg.regime_id,
      action: { type: 'MARK_AS_PERSONAL_TIME', from: seg.start_time, to: seg.end_time },
    }
    createOverride.mutate(req)
  }

  const handleReassign = (seg: DailyDigestSegment, targetRegimeId: string) => {
    const req: CreateOverrideRequest = {
      segment_id: seg.segment_id,
      original_regime_id: seg.regime_id,
      action: { type: 'REASSIGN_REGIME', target_regime_id: targetRegimeId },
    }
    createOverride.mutate(req)
  }

  const handleBulkMarkPersonal = () => {
    for (const seg of segments) {
      if (!overrideMap.has(seg.segment_id)) {
        handleMarkAsPersonal(seg)
      }
    }
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* Header */}
      <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('recalibration.title')}</h1>

        <Button variant="primary" size="sm" onClick={() => recluster.mutate()} disabled={recluster.isPending}>
          {recluster.isPending ? (
            <>
              <Spinner size="sm" />
              <span className="ml-2">{t('recalibration.reclustering')}</span>
            </>
          ) : (
            <>
              <RefreshCw className="mr-2 h-4 w-4" />
              {t('recalibration.triggerRecluster')}
            </>
          )}
        </Button>
      </div>

      {/* Controls */}
      <Card padding="md">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <DateRangePicker onRangeChange={handleRangeChange} />
          <Button
            variant="secondary"
            size="sm"
            onClick={handleBulkMarkPersonal}
            disabled={segments.length === 0 || createOverride.isPending}
          >
            {t('recalibration.markRangePersonal')}
          </Button>
        </div>
      </Card>

      {/* Segment list */}
      <Card padding="md">
        <h2 className={cn(typography.h3, colors.text.primary, 'mb-4')}>Segments</h2>

        {segmentsLoading && (
          <div className="space-y-2">
            <Skeleton className="h-12 w-full" />
            <Skeleton className="h-12 w-full" />
            <Skeleton className="h-12 w-full" />
          </div>
        )}

        {!segmentsLoading && segments.length === 0 && (
          <EmptyState
            icon={<RefreshCw className="h-8 w-8" />}
            title={t('recalibration.noSegments')}
            description={t('recalibration.noSegments')}
          />
        )}

        {!segmentsLoading && segments.length > 0 && (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className={cn('border-DEFAULT border-b', colors.text.tertiary)}>
                  <th className="pr-4 pb-2 font-medium">Time</th>
                  <th className="pr-4 pb-2 font-medium">Duration</th>
                  <th className="pr-4 pb-2 font-medium">Regime</th>
                  <th className="pr-4 pb-2 font-medium">App</th>
                  <th className="pb-2 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody>
                {segments.map((seg) => {
                  const override = overrideMap.get(seg.segment_id)
                  const isOverridden = !!override
                  return (
                    <tr key={seg.segment_id} className="border-DEFAULT border-b last:border-b-0">
                      <td className={cn('py-2 pr-4 text-xs', colors.text.secondary)}>
                        {formatTime(seg.start_time)} - {formatTime(seg.end_time)}
                      </td>
                      <td className={cn('py-2 pr-4 text-xs', colors.text.secondary)}>{seg.duration_mins}m</td>
                      <td className="py-2 pr-4">
                        <div className="flex items-center gap-2">
                          <span
                            className="inline-block h-2.5 w-2.5 rounded-full"
                            style={{ backgroundColor: seg.regime_color }}
                          />
                          <span className={cn('text-xs', colors.text.primary, isOverridden ? 'line-through' : '')}>
                            {seg.regime_label}
                          </span>
                          {override && (
                            <Badge color="warning" size="sm">
                              {getActionLabel(override, t)}
                            </Badge>
                          )}
                        </div>
                      </td>
                      <td className={cn('py-2 pr-4 text-xs', colors.text.secondary)}>{seg.dominant_app}</td>
                      <td className="py-2">
                        <div className="flex items-center gap-2">
                          {!isOverridden && (
                            <>
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => handleMarkAsPersonal(seg)}
                                disabled={createOverride.isPending}
                              >
                                {t('recalibration.markAsPersonalTime')}
                              </Button>
                              <Select
                                selectSize="sm"
                                defaultValue=""
                                onChange={(e) => {
                                  if (e.target.value) handleReassign(seg, e.target.value)
                                }}
                                className="w-36"
                              >
                                <option value="" disabled>
                                  {t('recalibration.changeRegimeTo')}
                                </option>
                                {REGIME_OPTIONS.filter((r) => r.id !== seg.regime_id).map((r) => (
                                  <option key={r.id} value={r.id}>
                                    {r.label}
                                  </option>
                                ))}
                              </Select>
                            </>
                          )}
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Override history */}
      <Card padding="md">
        <h2 className={cn(typography.h3, colors.text.primary, 'mb-4')}>{t('recalibration.overrideHistory')}</h2>

        {overridesLoading && (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        )}

        {!overridesLoading && (!overrides || overrides.length === 0) && (
          <p className={cn('py-4 text-center text-sm', colors.text.secondary)}>{t('recalibration.noOverrides')}</p>
        )}

        {!overridesLoading && overrides && overrides.length > 0 && (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className={cn('border-DEFAULT border-b', colors.text.tertiary)}>
                  <th className="pr-4 pb-2 font-medium">Segment</th>
                  <th className="pr-4 pb-2 font-medium">Original</th>
                  <th className="pr-4 pb-2 font-medium">Action</th>
                  <th className="pr-4 pb-2 font-medium">Created</th>
                  <th className="pb-2 font-medium" />
                </tr>
              </thead>
              <tbody>
                {overrides.map((override) => (
                  <tr key={override.override_id} className="border-DEFAULT border-b last:border-b-0">
                    <td className={cn('py-2 pr-4 font-mono text-xs', colors.text.secondary)}>
                      {override.segment_id.slice(0, 8)}...
                    </td>
                    <td className={cn('py-2 pr-4 text-xs', colors.text.secondary)}>
                      {override.original_regime_id ?? '-'}
                    </td>
                    <td className="py-2 pr-4">
                      <Badge color="info" size="sm">
                        {getActionLabel(override, t)}
                      </Badge>
                    </td>
                    <td className={cn('py-2 pr-4 text-xs', colors.text.tertiary)}>
                      {formatDateTime(override.created_at)}
                    </td>
                    <td className="py-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => deleteOverride.mutate(override.override_id)}
                        disabled={deleteOverride.isPending}
                        aria-label={t('recalibration.undo')}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        <span className="ml-1">{t('recalibration.undo')}</span>
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  )
}
