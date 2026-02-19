/**
 * 개인정보 관리 페이지
 *
 * 데이터 확인, 삭제, 백업/복원 기능
 */
import { useState, useRef, ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { FileText, Camera, BarChart3, HardDrive, Calendar } from 'lucide-react'
import {
  deleteDataRange,
  deleteAllData,
  fetchStorageStats,
  downloadBackup,
  restoreBackup,
  downloadBlob,
  DeleteRangeRequest,
  DeleteResult,
  BackupParams,
  BackupArchive,
  RestoreResult,
} from '../api/client'
import { Card, CardTitle, Input, Button, Spinner } from '../components/ui'
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
  if (!isOpen) return null

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <Card variant="default" padding="lg" className="max-w-md w-full mx-4 shadow-xl">
        <CardTitle className={`mb-2 ${isDangerous ? 'text-red-400' : ''}`}>
          {title}
        </CardTitle>
        <p className="text-slate-600 dark:text-slate-300 mb-6 whitespace-pre-line">{message}</p>
        <div className="flex justify-end space-x-3">
          <Button variant="secondary" onClick={onCancel}>
            {t('privacy.cancel')}
          </Button>
          <Button
            variant={isDangerous ? 'danger' : 'primary'}
            onClick={onConfirm}
          >
            {confirmText}
          </Button>
        </div>
      </Card>
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

  // 날짜 범위 삭제 폼 상태
  const [fromDate, setFromDate] = useState('')
  const [toDate, setToDate] = useState('')
  const [selectedDataTypes, setSelectedDataTypes] = useState<DataType[]>([])

  // 백업/복원 상태
  const [backupOptions, setBackupOptions] = useState<BackupParams>({
    include_settings: true,
    include_tags: true,
    include_events: false,
    include_frames: false,
  })
  const [restoreResult, setRestoreResult] = useState<RestoreResult | null>(null)

  const { data: storageStats, isLoading } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: fetchStorageStats,
  })

  const deleteRangeMutation = useMutation({
    mutationFn: deleteDataRange,
    onSuccess: (result) => {
      setDeleteResult(result)
      queryClient.invalidateQueries({ queryKey: ['storage-stats'] })
      queryClient.invalidateQueries({ queryKey: ['frames'] })
      queryClient.invalidateQueries({ queryKey: ['metrics'] })
      setShowDeleteRangeModal(false)
      setFromDate('')
      setToDate('')
      setSelectedDataTypes([])
    },
  })

  const deleteAllMutation = useMutation({
    mutationFn: deleteAllData,
    onSuccess: (result) => {
      setDeleteResult(result)
      queryClient.invalidateQueries()
      setShowDeleteAllModal(false)
    },
  })

  // 백업 다운로드 mutation
  const backupMutation = useMutation({
    mutationFn: downloadBackup,
    onSuccess: (blob) => {
      const now = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '_')
      downloadBlob(blob, `oneshim_backup_${now}.json`)
    },
  })

  // 복원 mutation
  const restoreMutation = useMutation({
    mutationFn: restoreBackup,
    onSuccess: (result) => {
      setRestoreResult(result)
      queryClient.invalidateQueries()
    },
  })

  const handleDataTypeToggle = (type: DataType) => {
    setSelectedDataTypes((prev) =>
      prev.includes(type) ? prev.filter((dt) => dt !== type) : [...prev, type]
    )
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

    try {
      const text = await file.text()
      const archive: BackupArchive = JSON.parse(text)

      // 버전 확인
      if (!archive.metadata?.version) {
        alert(t('backup.restoreFailed'))
        return
      }

      restoreMutation.mutate(archive)
    } catch {
      alert(t('backup.restoreFailed'))
    }

    // 파일 입력 초기화
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
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className="text-teal-500" />
        <span className="ml-3 text-slate-600 dark:text-slate-400">{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 헤더 */}
      <div>
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('privacy.title')}</h1>
        <p className="mt-1 text-slate-600 dark:text-slate-400">{t('privacy.subtitle')}</p>
      </div>

      {/* 삭제 결과 메시지 */}
      {deleteResult && (
        <div className="bg-green-500/20 border border-green-500 text-green-600 dark:text-green-400 p-4 rounded-lg">
          <div className="font-medium">{deleteResult.message}</div>
          <div className="mt-2 text-sm space-y-1">
            {deleteResult.events_deleted > 0 && <div>{t('privacy.deleteResult.events', { count: deleteResult.events_deleted })}</div>}
            {deleteResult.frames_deleted > 0 && <div>{t('privacy.deleteResult.frames', { count: deleteResult.frames_deleted })}</div>}
            {deleteResult.metrics_deleted > 0 && <div>{t('privacy.deleteResult.metrics', { count: deleteResult.metrics_deleted })}</div>}
            {deleteResult.process_snapshots_deleted > 0 && <div>{t('privacy.deleteResult.process_snapshots', { count: deleteResult.process_snapshots_deleted })}</div>}
            {deleteResult.idle_periods_deleted > 0 && <div>{t('privacy.deleteResult.idle_periods', { count: deleteResult.idle_periods_deleted })}</div>}
          </div>
          <button
            onClick={() => setDeleteResult(null)}
            className="mt-3 text-sm underline hover:no-underline"
          >
            {t('privacy.close')}
          </button>
        </div>
      )}

      {/* 현재 저장된 데이터 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.currentData')}</CardTitle>
        {storageStats && (
          <>
            <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
              <DataCard label={t('privacy.eventsLabel')} value={formatNumber(storageStats.event_count)} icon={<FileText className="w-4 h-4" />} />
              <DataCard label={t('privacy.screenshotsLabel')} value={formatNumber(storageStats.frame_count)} icon={<Camera className="w-4 h-4" />} />
              <DataCard label={t('privacy.metricsLabel')} value={formatNumber(storageStats.metric_count)} icon={<BarChart3 className="w-4 h-4" />} />
              <DataCard
                label={t('privacy.storageSizeLabel')}
                value={formatBytes(storageStats.total_size_bytes)}
                icon={<HardDrive className="w-4 h-4" />}
              />
              <DataCard label={t('privacy.dataRangeLabel')} value={getDateRangeText()} icon={<Calendar className="w-4 h-4" />} small />
            </div>
            <div className="mt-4 text-sm text-slate-500 dark:text-slate-500">
              {t('settings.dbSize')}: {formatBytes(storageStats.db_size_bytes)} / {t('settings.frameSize')}: {formatBytes(storageStats.frames_size_bytes)}
            </div>
          </>
        )}
      </Card>

      {/* 날짜 범위로 삭제 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.deleteByRangeTitle')}</CardTitle>
        <p className="text-slate-600 dark:text-slate-400 text-sm mb-4">{t('privacy.deleteByRangeDesc')}</p>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('privacy.startDate')}</label>
            <Input
              type="date"
              value={fromDate}
              onChange={(e) => setFromDate(e.target.value)}
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('privacy.endDate')}</label>
            <Input
              type="date"
              value={toDate}
              onChange={(e) => setToDate(e.target.value)}
            />
          </div>
        </div>

        <div className="mb-4">
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('privacy.dataTypesHint')}</label>
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

      {/* 전체 데이터 삭제 */}
      <Card variant="danger" padding="lg">
        <CardTitle className="mb-2 text-red-600 dark:text-red-400">{t('privacy.deleteAllTitle')}</CardTitle>
        <p className="text-slate-600 dark:text-slate-400 text-sm mb-4">
          {t('privacy.deleteAllDesc')}
        </p>
        <Button
          variant="danger"
          onClick={() => setShowDeleteAllModal(true)}
          isLoading={deleteAllMutation.isPending}
        >
          {deleteAllMutation.isPending ? t('privacy.deleting') : t('privacy.deleteAllButton')}
        </Button>
      </Card>

      {/* 백업/복원 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('backup.title')}</CardTitle>
        <p className="text-slate-600 dark:text-slate-400 text-sm mb-4">
          {t('backup.description')}
        </p>

        {/* 복원 결과 메시지 */}
        {restoreResult && (
          <div className={`mb-4 p-4 rounded-lg ${
            restoreResult.success
              ? 'bg-green-500/20 border border-green-500 text-green-600 dark:text-green-400'
              : 'bg-red-500/20 border border-red-500 text-red-600 dark:text-red-400'
          }`}>
            <div className="font-medium">
              {restoreResult.success ? t('backup.restoreSuccess') : t('backup.restoreFailed')}
            </div>
            <div className="mt-2 text-sm space-y-1">
              {restoreResult.restored.settings && <div>{t('backup.settingsRestored')}</div>}
              {restoreResult.restored.tags > 0 && <div>{t('backup.tagsRestored', { count: restoreResult.restored.tags })}</div>}
              {restoreResult.restored.frame_tags > 0 && <div>{t('backup.frameTagsRestored', { count: restoreResult.restored.frame_tags })}</div>}
              {restoreResult.restored.events > 0 && <div>{t('backup.eventsRestored', { count: restoreResult.restored.events })}</div>}
              {restoreResult.restored.frames > 0 && <div>{t('backup.framesRestored', { count: restoreResult.restored.frames })}</div>}
            </div>
            {restoreResult.errors.length > 0 && (
              <div className="mt-2 text-sm">
                <div className="font-medium">{t('backup.errors')}:</div>
                <ul className="list-disc list-inside">
                  {restoreResult.errors.slice(0, 5).map((err, i) => (
                    <li key={i}>{err}</li>
                  ))}
                  {restoreResult.errors.length > 5 && (
                    <li>...{t('backup.moreErrors', { count: restoreResult.errors.length - 5 })}</li>
                  )}
                </ul>
              </div>
            )}
            <button
              onClick={() => setRestoreResult(null)}
              className="mt-3 text-sm underline hover:no-underline"
            >
              {t('common.close')}
            </button>
          </div>
        )}

        {/* 백업 옵션 */}
        <div className="mb-4">
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
            {t('backup.includeData')}
          </label>
          <div className="flex flex-wrap gap-2">
            <Button
              variant={backupOptions.include_settings ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions(prev => ({ ...prev, include_settings: !prev.include_settings }))}
            >
              {t('backup.settings')}
            </Button>
            <Button
              variant={backupOptions.include_tags ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions(prev => ({ ...prev, include_tags: !prev.include_tags }))}
            >
              {t('backup.tags')}
            </Button>
            <Button
              variant={backupOptions.include_events ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions(prev => ({ ...prev, include_events: !prev.include_events }))}
            >
              {t('backup.events')}
            </Button>
            <Button
              variant={backupOptions.include_frames ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setBackupOptions(prev => ({ ...prev, include_frames: !prev.include_frames }))}
            >
              {t('backup.frames')}
            </Button>
          </div>
          <p className="mt-2 text-xs text-slate-500 dark:text-slate-500">
            {t('backup.optionsHint')}
          </p>
        </div>

        {/* 백업/복원 버튼 */}
        <div className="flex flex-wrap gap-3">
          <Button
            variant="primary"
            onClick={handleBackup}
            isLoading={backupMutation.isPending}
          >
            {backupMutation.isPending ? t('backup.creating') : t('backup.download')}
          </Button>

          <div>
            <input
              ref={fileInputRef}
              type="file"
              accept=".json"
              onChange={handleRestoreFile}
              className="hidden"
            />
            <Button
              variant="secondary"
              onClick={() => fileInputRef.current?.click()}
              isLoading={restoreMutation.isPending}
            >
              {restoreMutation.isPending ? t('backup.restoring') : t('backup.restore')}
            </Button>
          </div>
        </div>

        {/* 백업 실패 메시지 */}
        {backupMutation.isError && (
          <div className="mt-3 text-sm text-red-600 dark:text-red-400">
            {t('backup.downloadFailed')}: {(backupMutation.error as Error).message}
          </div>
        )}
        {restoreMutation.isError && (
          <div className="mt-3 text-sm text-red-600 dark:text-red-400">
            {t('backup.restoreFailed')}: {(restoreMutation.error as Error).message}
          </div>
        )}
      </Card>

      {/* 개인정보 안내 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('privacy.dataInfoTitle')}</CardTitle>
        <div className="space-y-3 text-sm text-slate-700 dark:text-slate-300">
          <div className="flex items-start space-x-2">
            <span className="text-teal-500 dark:text-teal-400">✓</span>
            <span>{t('privacy.dataInfo1')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-teal-500 dark:text-teal-400">✓</span>
            <span>{t('privacy.dataInfo2')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-teal-500 dark:text-teal-400">✓</span>
            <span>{t('privacy.dataInfo3')}</span>
          </div>
          <div className="flex items-start space-x-2">
            <span className="text-teal-500 dark:text-teal-400">✓</span>
            <span>{t('privacy.dataInfo4')}</span>
          </div>
        </div>
      </Card>

      {/* 날짜 범위 삭제 확인 모달 */}
      <ConfirmModal
        isOpen={showDeleteRangeModal}
        title={t('privacy.confirmDeleteRange')}
        message={`${fromDate} ~ ${toDate} 기간의 데이터를 삭제합니다.\n\n삭제 대상: ${
          selectedDataTypes.length > 0
            ? selectedDataTypes.map((dt) => DATA_TYPE_LABELS[dt]).join(', ')
            : '모든 데이터 유형'
        }\n\n이 작업은 되돌릴 수 없습니다.`}
        confirmText={t('privacy.deleteRange')}
        isDangerous={false}
        onConfirm={handleDeleteRange}
        onCancel={() => setShowDeleteRangeModal(false)}
      />

      {/* 전체 삭제 확인 모달 */}
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
      <div className="flex items-center space-x-2 text-slate-600 dark:text-slate-400 text-sm">
        {icon}
        <span>{label}</span>
      </div>
      <div className={`font-bold text-slate-900 dark:text-white mt-1 ${small ? 'text-sm' : 'text-xl'}`}>{value}</div>
    </Card>
  )
}
