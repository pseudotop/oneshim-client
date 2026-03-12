/**
 *
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { BarChart3, Calendar, Camera, FileText, HardDrive } from 'lucide-react'
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type BackupArchive,
  type BackupParams,
  type DeleteRangeRequest,
  type DeleteResult,
  deleteAllData,
  deleteDataRange,
  downloadBackup,
  downloadBlob,
  fetchStorageStats,
  type RestoreResult,
  restoreBackup,
} from '../api/client'
import { Button, Card, CardTitle, Input, Spinner } from '../components/ui'
import { addToast } from '../hooks/useToast'
import { colors, elevation, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatBytes, formatNumber } from '../utils/formatters'

interface ConfirmModalProps {
  isOpen: boolean
  title: string
  message: string
  confirmText: string
  isDangerous: boolean
  onConfirm: () => void
  onCancel: () => void
}

function ConfirmModal({ isOpen, title, message, confirmText, isDangerous, onConfirm, onCancel }: ConfirmModalProps) {
  const { t } = useTranslation()
  const dialogRef = useRef<HTMLDivElement>(null)
  const previousFocusRef = useRef<Element | null>(null)
  const descriptionId = useId()

  useEffect(() => {
    if (!isOpen) return
    previousFocusRef.current = document.activeElement

    // 첫 번째 포커스 가능한 요소에 포커스
    const timer = setTimeout(() => {
      dialogRef.current?.querySelector<HTMLElement>('button')?.focus()
    }, 50)

    return () => {
      clearTimeout(timer)
      // 닫힐 때 이전 포커스 복원
      if (previousFocusRef.current instanceof HTMLElement) {
        previousFocusRef.current.focus()
      }
    }
  }, [isOpen])

  // Focus trap: Tab 키가 다이얼로그 밖으로 나가지 않도록
  useEffect(() => {
    if (!isOpen) return
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onCancel()
        return
      }
      if (e.key !== 'Tab') return
      const dialog = dialogRef.current
      if (!dialog) return
      const focusable = dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault()
          last.focus()
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault()
          first.focus()
        }
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [isOpen, onCancel])

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-describedby={descriptionId}
      >
        <Card variant="default" padding="lg" className={cn('mx-4 w-full max-w-md', elevation.dialog)}>
          <CardTitle className={`mb-2 ${isDangerous ? 'text-red-400' : ''}`}>{title}</CardTitle>
          <p id={descriptionId} className="mb-6 whitespace-pre-line text-content-secondary">{message}</p>
          <div className="flex justify-end space-x-3">
            <Button variant="secondary" onClick={onCancel}>
              {t('privacy.cancel')}
            </Button>
            <Button variant={isDangerous ? 'danger' : 'primary'} onClick={onConfirm}>
              {confirmText}
            </Button>
          </div>
        </Card>
      </div>
    </div>
  )
}

type DataType = 'events' | 'frames' | 'metrics' | 'processes' | 'idle'

export default function Privacy() {
  const { t } = useTranslation()
  const DATA_TYPE_LABELS: Record<DataType, string> = {
    events: t('privacy.dataTypes.events'),
    frames: t('privacy.dataTypes.frames'),
    metrics: t('privacy.dataTypes.metrics'),
    processes: t('privacy.dataTypes.process_snapshots'),
    idle: t('privacy.dataTypes.idle_periods'),
  }
  const queryClient = useQueryClient()
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [showDeleteRangeModal, setShowDeleteRangeModal] = useState(false)
  const [showDeleteAllModal, setShowDeleteAllModal] = useState(false)
  const [deleteResult, setDeleteResult] = useState<DeleteResult | null>(null)

  const [fromDate, setFromDate] = useState('')
  const [toDate, setToDate] = useState('')
  const [selectedDataTypes, setSelectedDataTypes] = useState<DataType[]>([])

  const [backupOptions, setBackupOptions] = useState<BackupParams>({
    include_settings: true,
    include_tags: true,
    include_events: false,
    include_frames: false,
  })
  const [restoreResult, setRestoreResult] = useState<RestoreResult | null>(null)
  const [restoreError, setRestoreError] = useState<string | null>(null)

  const { data: storageStats, isLoading } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: fetchStorageStats,
  })

  const deleteRangeMutation = useMutation({
    mutationFn: deleteDataRange,
    onSuccess: (result) => {
      setDeleteResult(result)
      addToast('success', result.message)
      queryClient.invalidateQueries({ queryKey: ['storage-stats'] })
      queryClient.invalidateQueries({ queryKey: ['frames'] })
      queryClient.invalidateQueries({ queryKey: ['metrics'] })
      setShowDeleteRangeModal(false)
      setFromDate('')
      setToDate('')
      setSelectedDataTypes([])
    },
    onError: (error: Error) => {
      addToast('error', error.message)
    },
  })

  const deleteAllMutation = useMutation({
    mutationFn: deleteAllData,
    onSuccess: (result) => {
      setDeleteResult(result)
      addToast('success', result.message)
      queryClient.invalidateQueries()
      setShowDeleteAllModal(false)
    },
    onError: (error: Error) => {
      addToast('error', error.message)
    },
  })

  const backupMutation = useMutation({
    mutationFn: downloadBackup,
    onSuccess: (blob) => {
      const now = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '_')
      downloadBlob(blob, `oneshim_backup_${now}.json`)
      addToast('success', t('backup.downloadComplete'))
    },
    onError: (error: Error) => {
      addToast('error', `${t('backup.downloadFailed')}: ${error.message}`)
    },
  })

  const restoreMutation = useMutation({
    mutationFn: restoreBackup,
    onSuccess: (result) => {
      setRestoreResult(result)
      addToast(result.success ? 'success' : 'error', result.success ? t('backup.restoreSuccess') : t('backup.restoreFailed'))
      queryClient.invalidateQueries()
    },
    onError: (error: Error) => {
      addToast('error', `${t('backup.restoreFailed')}: ${error.message}`)
    },
  })

  const handleDataTypeToggle = (type: DataType) => {
    setSelectedDataTypes((prev) => (prev.includes(type) ? prev.filter((dt) => dt !== type) : [...prev, type]))
  }

  const handleDeleteRange = () => {
    if (!fromDate || !toDate) return

    const request: DeleteRangeRequest = {
      from: fromDate,
      to: toDate,
      data_types: selectedDataTypes.length > 0 ? selectedDataTypes : undefined,
    }

    deleteRangeMutation.mutate(request)
  }

  const handleDeleteAll = () => {
    deleteAllMutation.mutate()
  }

  const handleBackup = () => {
    backupMutation.mutate(backupOptions)
  }

  const handleRestoreFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return

    setRestoreError(null)

    try {
      const text = await file.text()
      const archive: BackupArchive = JSON.parse(text)

      if (!archive.metadata?.version) {
        setRestoreError(t('backup.restoreFailed'))
        return
      }

      restoreMutation.mutate(archive)
    } catch {
      setRestoreError(t('backup.restoreFailed'))
    }

    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }

  const getDateRangeText = () => {
    if (storageStats?.oldest_data_date && storageStats?.newest_data_date) {
      return `${storageStats.oldest_data_date.split('T')[0]} ~ ${storageStats.newest_data_date.split('T')[0]}`
    }
    return t('common.noData')
  }

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className="text-accent-teal" />
        <span className="ml-3 text-content-secondary">{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div>
        <h1 className={cn(typography.h1, colors.text.primary)}>{t('privacy.title')}</h1>
        <p className="mt-1 text-content-secondary">{t('privacy.subtitle')}</p>
      </div>

      {/* UI note */}
      {deleteResult && (
        <div className="rounded-lg border border-status-connected bg-semantic-success/20 p-4 text-semantic-success">
          <div className="font-medium">{deleteResult.message}</div>
          <div className="mt-2 space-y-1 text-sm">
            {deleteResult.events_deleted > 0 && (
              <div>{t('privacy.deleteResult.events', { count: deleteResult.events_deleted })}</div>
            )}
            {deleteResult.frames_deleted > 0 && (
              <div>{t('privacy.deleteResult.frames', { count: deleteResult.frames_deleted })}</div>
            )}
            {deleteResult.metrics_deleted > 0 && (
              <div>{t('privacy.deleteResult.metrics', { count: deleteResult.metrics_deleted })}</div>
            )}
            {deleteResult.process_snapshots_deleted > 0 && (
              <div>
                {t('privacy.deleteResult.process_snapshots', { count: deleteResult.process_snapshots_deleted })}
              </div>
            )}
            {deleteResult.idle_periods_deleted > 0 && (
              <div>{t('privacy.deleteResult.idle_periods', { count: deleteResult.idle_periods_deleted })}</div>
            )}
          </div>
          <button
            type="button"
            onClick={() => setDeleteResult(null)}
            className="mt-3 text-sm underline hover:no-underline"
          >
            {t('privacy.close')}
          </button>
        </div>
      )}

      {/* UI note */}
      <Card id="section-data" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.currentData')}</CardTitle>
        {storageStats && (
          <>
            <div className="grid grid-cols-2 gap-4 md:grid-cols-5">
              <DataCard
                label={t('privacy.eventsLabel')}
                value={formatNumber(storageStats.event_count)}
                icon={<FileText className="h-4 w-4" />}
              />
              <DataCard
                label={t('privacy.screenshotsLabel')}
                value={formatNumber(storageStats.frame_count)}
                icon={<Camera className="h-4 w-4" />}
              />
              <DataCard
                label={t('privacy.metricsLabel')}
                value={formatNumber(storageStats.metric_count)}
                icon={<BarChart3 className="h-4 w-4" />}
              />
              <DataCard
                label={t('privacy.storageSizeLabel')}
                value={formatBytes(storageStats.total_size_bytes)}
                icon={<HardDrive className="h-4 w-4" />}
              />
              <DataCard
                label={t('privacy.dataRangeLabel')}
                value={getDateRangeText()}
                icon={<Calendar className="h-4 w-4" />}
                small
              />
            </div>
            <div className="mt-4 text-content-tertiary text-sm">
              {t('settings.dbSize')}: {formatBytes(storageStats.db_size_bytes)} / {t('settings.frameSize')}:{' '}
              {formatBytes(storageStats.frames_size_bytes)}
            </div>
          </>
        )}
      </Card>

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.deleteByRangeTitle')}</CardTitle>
        <p className="mb-4 text-content-secondary text-sm">{t('privacy.deleteByRangeDesc')}</p>

        <div className="mb-4 grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <label htmlFor="privacy-start-date" className="mb-2 block font-medium text-content-strong text-sm">
              {t('privacy.startDate')}
            </label>
            <Input id="privacy-start-date" type="date" value={fromDate} onChange={(e) => setFromDate(e.target.value)} />
          </div>
          <div>
            <label htmlFor="privacy-end-date" className="mb-2 block font-medium text-content-strong text-sm">
              {t('privacy.endDate')}
            </label>
            <Input id="privacy-end-date" type="date" value={toDate} onChange={(e) => setToDate(e.target.value)} />
          </div>
        </div>

        <div className="mb-4">
          <span className="mb-2 block font-medium text-content-strong text-sm">{t('privacy.dataTypesHint')}</span>
          <div className="flex flex-wrap gap-2">
            {(Object.entries(DATA_TYPE_LABELS) as [DataType, string][]).map(([type, label]) => (
              <Button
                key={type}
                variant={selectedDataTypes.includes(type) ? 'primary' : 'secondary'}
                size="sm"
                onClick={() => handleDataTypeToggle(type)}
              >
                {label}
              </Button>
            ))}
          </div>
        </div>

        <Button
          variant="warning"
          onClick={() => setShowDeleteRangeModal(true)}
          disabled={!fromDate || !toDate}
          isLoading={deleteRangeMutation.isPending}
        >
          {deleteRangeMutation.isPending ? t('privacy.deleting') : t('privacy.deleteRangeButton')}
        </Button>
      </Card>

      {/* UI note */}
      <Card id="section-consent" variant="danger" padding="lg">
        <CardTitle className="mb-2 text-accent-red">{t('privacy.deleteAllTitle')}</CardTitle>
        <p className="mb-4 text-content-secondary text-sm">{t('privacy.deleteAllDesc')}</p>
        <Button variant="danger" onClick={() => setShowDeleteAllModal(true)} isLoading={deleteAllMutation.isPending}>
          {deleteAllMutation.isPending ? t('privacy.deleting') : t('privacy.deleteAllButton')}
        </Button>
      </Card>

      {/* UI note */}
      <Card id="section-export" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('backup.title')}</CardTitle>
        <p className="mb-4 text-content-secondary text-sm">{t('backup.description')}</p>

        {/* UI note */}
        {restoreError && (
          <div
            role="alert"
            className="mb-4 rounded-lg border border-status-error bg-semantic-error/20 p-4 text-semantic-error"
          >
            <div className="font-medium">{restoreError}</div>
            <button
              type="button"
              className="mt-2 text-sm underline"
              onClick={() => setRestoreError(null)}
            >
              {t('common.dismiss', 'Dismiss')}
            </button>
          </div>
        )}

        {restoreResult && (
          <div
            className={`mb-4 rounded-lg p-4 ${
              restoreResult.success
                ? 'border border-status-connected bg-semantic-success/20 text-semantic-success'
                : 'border border-status-error bg-semantic-error/20 text-semantic-error'
            }`}
          >
            <div className="font-medium">
              {restoreResult.success ? t('backup.restoreSuccess') : t('backup.restoreFailed')}
            </div>
            <div className="mt-2 space-y-1 text-sm">
              {restoreResult.restored.settings && <div>{t('backup.settingsRestored')}</div>}
              {restoreResult.restored.tags > 0 && (
                <div>{t('backup.tagsRestored', { count: restoreResult.restored.tags })}</div>
              )}
              {restoreResult.restored.frame_tags > 0 && (
                <div>{t('backup.frameTagsRestored', { count: restoreResult.restored.frame_tags })}</div>
              )}
              {restoreResult.restored.events > 0 && (
                <div>{t('backup.eventsRestored', { count: restoreResult.restored.events })}</div>
              )}
              {restoreResult.restored.frames > 0 && (
                <div>{t('backup.framesRestored', { count: restoreResult.restored.frames })}</div>
              )}
            </div>
            {restoreResult.errors.length > 0 && (
              <div className="mt-2 text-sm">
                <div className="font-medium">{t('backup.errors')}:</div>
                <ul className="list-inside list-disc">
                  {restoreResult.errors.slice(0, 5).map((err) => (
                    <li key={err}>{err}</li>
                  ))}
                  {restoreResult.errors.length > 5 && (
                    <li>...{t('backup.moreErrors', { count: restoreResult.errors.length - 5 })}</li>
                  )}
                </ul>
              </div>
            )}
            <button
              type="button"
              onClick={() => setRestoreResult(null)}
              className="mt-3 text-sm underline hover:no-underline"
            >
              {t('common.close')}
            </button>
          </div>
        )}

        {/* UI note */}
        <div className="mb-4">
          <span className="mb-2 block font-medium text-content-strong text-sm">{t('backup.includeData')}</span>
          <div className="flex flex-wrap gap-2">
            <Button
              variant={backupOptions.include_settings ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions((prev) => ({ ...prev, include_settings: !prev.include_settings }))}
            >
              {t('backup.settings')}
            </Button>
            <Button
              variant={backupOptions.include_tags ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions((prev) => ({ ...prev, include_tags: !prev.include_tags }))}
            >
              {t('backup.tags')}
            </Button>
            <Button
              variant={backupOptions.include_events ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions((prev) => ({ ...prev, include_events: !prev.include_events }))}
            >
              {t('backup.events')}
            </Button>
            <Button
              variant={backupOptions.include_frames ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions((prev) => ({ ...prev, include_frames: !prev.include_frames }))}
            >
              {t('backup.frames')}
            </Button>
          </div>
          <p className="mt-2 text-content-tertiary text-xs">{t('backup.optionsHint')}</p>
        </div>

        {/* UI note */}
        <div className="flex flex-wrap gap-3">
          <Button variant="primary" onClick={handleBackup} isLoading={backupMutation.isPending}>
            {backupMutation.isPending ? t('backup.creating') : t('backup.download')}
          </Button>

          <div>
            <input ref={fileInputRef} type="file" accept=".json" onChange={handleRestoreFile} className="hidden" />
            <Button
              variant="secondary"
              onClick={() => fileInputRef.current?.click()}
              isLoading={restoreMutation.isPending}
            >
              {restoreMutation.isPending ? t('backup.restoring') : t('backup.restore')}
            </Button>
          </div>
        </div>

        {/* UI note */}
        {backupMutation.isError && (
          <div className="mt-3 text-accent-red text-sm">
            {t('backup.downloadFailed')}: {(backupMutation.error as Error).message}
          </div>
        )}
        {restoreMutation.isError && (
          <div className="mt-3 text-accent-red text-sm">
            {t('backup.restoreFailed')}: {(restoreMutation.error as Error).message}
          </div>
        )}
      </Card>

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.dataInfoTitle')}</CardTitle>
        <div className="space-y-3 text-content-strong text-sm">
          <div className="flex items-start space-x-2">
            <span className="text-accent-teal">✓</span>
            <span>{t('privacy.dataInfo1')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-accent-teal">✓</span>
            <span>{t('privacy.dataInfo2')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-accent-teal">✓</span>
            <span>{t('privacy.dataInfo3')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-accent-teal">✓</span>
            <span>{t('privacy.dataInfo4')}</span>
          </div>
        </div>
      </Card>

      {/* UI note */}
      <ConfirmModal
        isOpen={showDeleteRangeModal}
        title={t('privacy.confirmDeleteRange')}
        message={t('privacy.confirmDeleteRangeMsg', {
          fromDate,
          toDate,
          dataTypes: selectedDataTypes.length > 0
            ? selectedDataTypes.map((dt) => DATA_TYPE_LABELS[dt]).join(', ')
            : t('privacy.allDataTypes', 'All data types'),
          defaultValue: 'Delete data from {{fromDate}} to {{toDate}}.\n\nTarget: {{dataTypes}}\n\nThis action cannot be undone.',
        })}
        confirmText={t('privacy.deleteRange')}
        isDangerous={false}
        onConfirm={handleDeleteRange}
        onCancel={() => setShowDeleteRangeModal(false)}
      />

      {/* UI note */}
      <ConfirmModal
        isOpen={showDeleteAllModal}
        title={t('privacy.confirmDeleteAll')}
        message={t('privacy.confirmDeleteAllMsg')}
        confirmText={t('privacy.deleteAllButton')}
        isDangerous={true}
        onConfirm={handleDeleteAll}
        onCancel={() => setShowDeleteAllModal(false)}
      />
    </div>
  )
}

interface DataCardProps {
  label: string
  value: string
  icon: ReactNode
  small?: boolean
}

function DataCard({ label, value, icon, small }: DataCardProps) {
  return (
    <Card variant="elevated" padding="md">
      <div className="flex items-center space-x-2 text-content-secondary text-sm">
        {icon}
        <span>{label}</span>
      </div>
      <div className={`mt-1 font-bold text-content ${small ? 'text-sm' : 'text-xl'}`}>{value}</div>
    </Card>
  )
}
