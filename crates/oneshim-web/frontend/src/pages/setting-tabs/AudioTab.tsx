import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Spinner } from '../../components/ui'
import type { AudioSettings } from '../../api/contracts'
import { cn } from '../../utils/cn'
import { colors, radius, typography } from '../../styles/tokens'

const ipc = async <T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

type ModelSize = 'tiny' | 'base' | 'small' | 'medium'
type AudioStatusResponse = {
  enabled: boolean
  selected_model: ModelSize
  model_status:
    | { state: 'not_installed' }
    | { state: 'downloading'; progress_pct: number | null; bytes_downloaded: number; total_bytes: number | null }
    | { state: 'ready'; path: string; size_bytes: number }
    | { state: 'error'; message: string }
  stt_provider_loaded: boolean
}

const MODEL_LABELS: Record<ModelSize, string> = {
  tiny: 'Tiny (~75 MB)',
  base: 'Base (~142 MB)',
  small: 'Small (~466 MB)',
  medium: 'Medium (~1.5 GB)',
}

interface AudioTabProps {
  formData: { audio?: AudioSettings } | null
  onAudioChange: (field: string, value: unknown) => void
}

export default function AudioTab({ formData, onAudioChange }: AudioTabProps) {
  const { t } = useTranslation()
  const [audioStatus, setAudioStatus] = useState<AudioStatusResponse | null>(null)
  const [downloading, setDownloading] = useState(false)

  const fetchStatus = useCallback(async () => {
    try {
      const status = await ipc<AudioStatusResponse>('get_audio_status')
      setAudioStatus(status)
      setDownloading(status.model_status.state === 'downloading')
    } catch {
      // Tauri not available (standalone mode)
    }
  }, [])

  useEffect(() => {
    fetchStatus()
  }, [fetchStatus])

  useEffect(() => {
    let cleanup: (() => void) | undefined
    ;(async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event')
        const unlisten1 = await listen<{ progress_pct: number | null; bytes_downloaded: number; total_bytes: number | null }>('audio-model-progress', (event) => {
          setAudioStatus((prev) =>
            prev
              ? {
                  ...prev,
                  model_status: {
                    state: 'downloading' as const,
                    progress_pct: event.payload.progress_pct,
                    bytes_downloaded: event.payload.bytes_downloaded,
                    total_bytes: event.payload.total_bytes,
                  },
                }
              : prev,
          )
        })
        const unlisten2 = await listen('audio-model-complete', () => {
          setDownloading(false)
          fetchStatus()
        })
        const unlisten3 = await listen('audio-model-error', () => {
          setDownloading(false)
          fetchStatus()
        })
        cleanup = () => {
          unlisten1()
          unlisten2()
          unlisten3()
        }
      } catch {
        // not in Tauri
      }
    })()
    return () => cleanup?.()
  }, [fetchStatus])

  const audio = formData?.audio
  const enabled = audio?.enabled ?? false
  const modelSize = (audio?.model_size ?? 'base') as ModelSize
  const language = audio?.language ?? 'auto'
  const modelState = audioStatus?.model_status?.state ?? 'not_installed'

  const handleDownload = async () => {
    setDownloading(true)
    try {
      await ipc('download_whisper_model', { modelSize })
    } catch {
      setDownloading(false)
      fetchStatus()
    }
  }

  const handleCancel = async () => {
    try {
      await ipc('cancel_model_download')
    } catch {
      // ignore
    }
  }

  const handleDelete = async () => {
    if (!confirm(t('settings.audio.delete_confirm', 'Delete the downloaded model? You can re-download it later.'))) return
    try {
      await ipc('delete_whisper_model', { modelSize })
      fetchStatus()
    } catch {
      // ignore
    }
  }

  const handleReload = async () => {
    try {
      await ipc('reload_stt_engine')
      fetchStatus()
    } catch {
      // ignore
    }
  }

  return (
    <div className="space-y-6">
      <h3 className={cn(typography.h3, colors.text.primary)}>
        {t('settings.audio.title', 'Audio & Speech-to-Text')}
      </h3>

      <label className="flex items-center gap-3">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onAudioChange('enabled', e.target.checked)}
          className="h-4 w-4"
        />
        <span className={colors.text.primary}>
          {t('settings.audio.enable', 'Enable audio capture and STT')}
        </span>
      </label>

      <div className="space-y-2">
        <label className={cn(typography.label, colors.text.secondary)}>
          {t('settings.audio.model', 'Whisper Model')}
        </label>
        <select
          value={modelSize}
          onChange={(e) => onAudioChange('model_size', e.target.value)}
          disabled={downloading}
          className={cn('w-full border bg-surface-base px-3 py-2 text-sm', radius.md, colors.text.primary)}
        >
          {(Object.entries(MODEL_LABELS) as [ModelSize, string][]).map(([key, label]) => (
            <option key={key} value={key}>
              {label}
            </option>
          ))}
        </select>
      </div>

      <div className="space-y-2">
        <span className={cn(typography.label, colors.text.secondary)}>
          {t('settings.audio.status', 'Model Status')}
        </span>
        <div className="flex items-center gap-3">
          {modelState === 'not_installed' && (
            <span className="rounded bg-neutral-200 px-2 py-0.5 text-xs text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300">
              {t('settings.audio.not_installed', 'Not installed')}
            </span>
          )}
          {modelState === 'downloading' && audioStatus?.model_status.state === 'downloading' && (
            <div className="flex items-center gap-2">
              <Spinner />
              <span className="text-sm">
                {audioStatus.model_status.progress_pct != null
                  ? `${audioStatus.model_status.progress_pct}%`
                  : t('settings.audio.downloading', 'Downloading...')}
              </span>
            </div>
          )}
          {modelState === 'ready' && (
            <span className="rounded bg-green-100 px-2 py-0.5 text-xs text-green-700 dark:bg-green-900 dark:text-green-300">
              {t('settings.audio.ready', 'Ready')}
            </span>
          )}
          {modelState === 'error' && audioStatus?.model_status.state === 'error' && (
            <span className="rounded bg-red-100 px-2 py-0.5 text-xs text-red-700 dark:bg-red-900 dark:text-red-300">
              {audioStatus.model_status.message}
            </span>
          )}
        </div>
      </div>

      <div className="flex gap-2">
        {downloading ? (
          <Button variant="secondary" size="sm" onClick={handleCancel}>
            {t('settings.audio.cancel', 'Cancel')}
          </Button>
        ) : (
          <Button variant="primary" size="sm" onClick={handleDownload} disabled={!enabled}>
            {modelState === 'ready'
              ? t('settings.audio.redownload', 'Re-download')
              : t('settings.audio.download', 'Download')}
          </Button>
        )}
        {modelState === 'ready' && !downloading && (
          <>
            <Button variant="secondary" size="sm" onClick={handleReload}>
              {t('settings.audio.reload', 'Reload Engine')}
            </Button>
            <Button variant="danger" size="sm" onClick={handleDelete}>
              {t('settings.audio.delete', 'Delete')}
            </Button>
          </>
        )}
      </div>

      <div className="space-y-2">
        <label className={cn(typography.label, colors.text.secondary)}>
          {t('settings.audio.language', 'STT Language')}
        </label>
        <select
          value={language}
          onChange={(e) => onAudioChange('language', e.target.value)}
          className={cn('w-full border bg-surface-base px-3 py-2 text-sm', radius.md, colors.text.primary)}
        >
          <option value="auto">Auto-detect</option>
          <option value="en">English</option>
          <option value="ko">한국어</option>
        </select>
      </div>
    </div>
  )
}
