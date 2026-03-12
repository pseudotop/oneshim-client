import type {
  AppSettings,
  AppUsage,
  AuditEntry,
  AutomationContracts,
  AutomationStats,
  AutomationStatus,
  BackupArchive,
  BackupParams,
  CreateTagRequest,
  DailySummary,
  DeleteRangeRequest,
  DeleteResult,
  Event,
  ExecuteIntentHintRequest,
  ExecuteIntentHintResponse,
  ExecuteSceneActionRequest,
  ExecuteSceneActionResponse,
  ExportDataType,
  ExportFormat,
  FocusMetricsResponse,
  Frame,
  HeatmapResponse,
  HourlyMetrics,
  IdlePeriod,
  Interruption,
  LocalSuggestion,
  PaginatedResponse,
  PoliciesInfo,
  PresetRunResult,
  ProcessSnapshot,
  ProviderModelsRequest,
  ProviderModelsResponse,
  ProviderPresetCatalog,
  ReportParams,
  ReportResponse,
  RestoreResult,
  SceneCalibrationReport,
  SearchParams,
  SearchResponse,
  Session,
  StorageStats,
  SuggestionFeedbackAction,
  SystemMetrics,
  Tag,
  TimelineParams,
  TimelineResponse,
  UiScene,
  UpdateAction,
  UpdateActionResponse,
  UpdateStatus,
  UpdateTagRequest,
  WorkflowPreset,
  WorkSession,
} from './contracts'
import { resolveApiUrl } from '../utils/api-base'
import { handleStandaloneRequest, isStandaloneModeEnabled } from './standalone'

export type * from './contracts'

const BASE_URL = '/api'

const DEFAULT_TIMEOUT_MS = 10_000
const MAX_RETRIES = 2

async function fetchWithRetry(url: string, options?: RequestInit, retries = MAX_RETRIES): Promise<Response> {
  const resolvedUrl = await resolveApiUrl(url)

  if (isStandaloneModeEnabled()) {
    const standaloneResponse = await handleStandaloneRequest(resolvedUrl, options)
    if (standaloneResponse) {
      return standaloneResponse
    }
  }

  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS)

  try {
    const response = await fetch(resolvedUrl, {
      ...options,
      signal: controller.signal,
    })
    if (response.status >= 500) {
      if (retries > 0) {
        await new Promise((r) => setTimeout(r, 1000 * (MAX_RETRIES - retries + 1)))
        return fetchWithRetry(url, options, retries - 1)
      }
      const standaloneResponse = await handleStandaloneRequest(resolvedUrl, options, true)
      if (standaloneResponse) {
        return standaloneResponse
      }
    }
    return response
  } catch (error) {
    if (retries > 0) {
      await new Promise((r) => setTimeout(r, 1000 * (MAX_RETRIES - retries + 1)))
      return fetchWithRetry(url, options, retries - 1)
    }
    const standaloneResponse = await handleStandaloneRequest(resolvedUrl, options, true)
    if (standaloneResponse) {
      return standaloneResponse
    }
    throw error
  } finally {
    clearTimeout(timeoutId)
  }
}

export async function fetchSummary(date?: string): Promise<DailySummary> {
  const params = new URLSearchParams()
  if (date) params.set('date', date)
  const res = await fetchWithRetry(`${BASE_URL}/stats/summary?${params}`)
  if (!res.ok) throw new Error('Summary query failed')
  return res.json()
}

export async function fetchMetrics(from?: string, to?: string, limit = 100): Promise<SystemMetrics[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/metrics?${params}`)
  if (!res.ok) throw new Error('Metrics query failed')
  return res.json()
}

export async function fetchHourlyMetrics(hours = 24): Promise<HourlyMetrics[]> {
  const res = await fetchWithRetry(`${BASE_URL}/metrics/hourly?hours=${hours}`)
  if (!res.ok) throw new Error('Hourly metrics query failed')
  return res.json()
}

export async function fetchProcesses(from?: string, to?: string, limit = 20): Promise<ProcessSnapshot[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/processes?${params}`)
  if (!res.ok) throw new Error('Process query failed')
  return res.json()
}

export async function fetchFrames(
  from?: string,
  to?: string,
  limit = 50,
  offset = 0,
): Promise<PaginatedResponse<Frame>> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  params.set('offset', String(offset))
  const res = await fetchWithRetry(`${BASE_URL}/frames?${params}`)
  if (!res.ok) throw new Error('frame query failure')
  return res.json()
}

export async function fetchEvents(
  from?: string,
  to?: string,
  limit = 100,
  offset = 0,
): Promise<PaginatedResponse<Event>> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  params.set('offset', String(offset))
  const res = await fetchWithRetry(`${BASE_URL}/events?${params}`)
  if (!res.ok) throw new Error('event query failure')
  return res.json()
}

export async function fetchIdlePeriods(from?: string, to?: string): Promise<IdlePeriod[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  const res = await fetchWithRetry(`${BASE_URL}/idle?${params}`)
  if (!res.ok) throw new Error('idle period query failure')
  return res.json()
}

export async function fetchSessions(): Promise<Session[]> {
  const res = await fetchWithRetry(`${BASE_URL}/sessions`)
  if (!res.ok) throw new Error('session query failure')
  return res.json()
}

export async function fetchAppUsage(date?: string): Promise<{ date: string; apps: AppUsage[] }> {
  const params = new URLSearchParams()
  if (date) params.set('date', date)
  const res = await fetchWithRetry(`${BASE_URL}/stats/apps?${params}`)
  if (!res.ok) throw new Error('App usage query failed')
  return res.json()
}

export async function fetchStorageStats(): Promise<StorageStats> {
  const res = await fetchWithRetry(`${BASE_URL}/storage/stats`)
  if (!res.ok) throw new Error('Storage stats query failed')
  return res.json()
}

export async function fetchSettings(): Promise<AppSettings> {
  const res = await fetchWithRetry(`${BASE_URL}/settings`)
  if (!res.ok) throw new Error('Settings query failed')
  return res.json()
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  const res = await fetchWithRetry(`${BASE_URL}/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to save settings' }))
    throw new Error(err.error || 'Failed to save settings')
  }
  return res.json()
}

export async function fetchProviderPresets(): Promise<ProviderPresetCatalog> {
  const res = await fetchWithRetry(`${BASE_URL}/ai/providers/presets`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Provider preset query failed' }))
    throw new Error(err.error || 'Provider preset query failed')
  }
  return res.json()
}

export async function discoverProviderModels(request: ProviderModelsRequest): Promise<ProviderModelsResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/ai/providers/models`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Model discovery failed' }))
    throw new Error(err.error || 'Model discovery failed')
  }
  return res.json()
}

export async function fetchUpdateStatus(): Promise<UpdateStatus> {
  const res = await fetchWithRetry(`${BASE_URL}/update/status`)
  if (!res.ok) throw new Error('update state query failure')
  return res.json()
}

export async function postUpdateAction(action: UpdateAction): Promise<UpdateActionResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/update/action`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action }),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Update operation failed' }))
    throw new Error(err.error || 'Update operation failed')
  }
  return res.json()
}

export async function deleteDataRange(request: DeleteRangeRequest): Promise<DeleteResult> {
  const res = await fetchWithRetry(`${BASE_URL}/data/range`, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to delete data' }))
    throw new Error(err.error || 'Failed to delete data')
  }
  return res.json()
}

export async function deleteAllData(): Promise<DeleteResult> {
  const res = await fetchWithRetry(`${BASE_URL}/data/all`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to delete all data' }))
    throw new Error(err.error || 'Failed to delete all data')
  }
  return res.json()
}

export async function search(params: SearchParams): Promise<SearchResponse>
export async function search(
  query: string,
  searchType?: 'all' | 'frames' | 'events',
  limit?: number,
  offset?: number,
  tagIds?: number[],
): Promise<SearchResponse>
export async function search(
  queryOrParams: string | SearchParams,
  searchType: 'all' | 'frames' | 'events' = 'all',
  limit = 50,
  offset = 0,
  tagIds?: number[],
): Promise<SearchResponse> {
  let query: string
  let type: 'all' | 'frames' | 'events'
  let lim: number
  let off: number
  let tags: number[] | undefined

  if (typeof queryOrParams === 'object') {
    query = queryOrParams.query
    type = queryOrParams.searchType ?? 'all'
    lim = queryOrParams.limit ?? 50
    off = queryOrParams.offset ?? 0
    tags = queryOrParams.tagIds
  } else {
    query = queryOrParams
    type = searchType
    lim = limit
    off = offset
    tags = tagIds
  }

  const params = new URLSearchParams()
  params.set('q', query)
  params.set('search_type', type)
  params.set('limit', String(lim))
  params.set('offset', String(off))
  if (tags && tags.length > 0) {
    params.set('tag_ids', tags.join(','))
  }
  const res = await fetchWithRetry(`${BASE_URL}/search?${params}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Search failed' }))
    throw new Error(err.error || 'Search failed')
  }
  return res.json()
}

export async function fetchHeatmap(days = 7): Promise<HeatmapResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/stats/heatmap?days=${days}`)
  if (!res.ok) throw new Error('Heatmap query failed')
  return res.json()
}

export async function exportData(
  dataType: ExportDataType,
  format: ExportFormat = 'json',
  from?: string,
  to?: string,
): Promise<Blob> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('format', format)

  const res = await fetchWithRetry(`${BASE_URL}/export/${dataType}?${params}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Viewer request failed' }))
    throw new Error(err.error || 'Viewer request failed')
  }
  return res.blob()
}

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

export async function fetchTags(): Promise<Tag[]> {
  const res = await fetchWithRetry(`${BASE_URL}/tags`)
  if (!res.ok) throw new Error('Tag query failed')
  return res.json()
}

export async function createTag(request: CreateTagRequest): Promise<Tag> {
  const res = await fetchWithRetry(`${BASE_URL}/tags`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to create tag' }))
    throw new Error(err.error || 'Failed to create tag')
  }
  return res.json()
}

export async function updateTag(tagId: number, request: UpdateTagRequest): Promise<Tag> {
  const res = await fetchWithRetry(`${BASE_URL}/tags/${tagId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to update tag' }))
    throw new Error(err.error || 'Failed to update tag')
  }
  return res.json()
}

export async function deleteTag(tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/tags/${tagId}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to delete tag' }))
    throw new Error(err.error || 'Failed to delete tag')
  }
}

export async function fetchFrameTags(frameId: number): Promise<Tag[]> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags`)
  if (!res.ok) throw new Error('frame Tag query failed')
  return res.json()
}

export async function addTagToFrame(frameId: number, tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags/${tagId}`, {
    method: 'POST',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to add tag' }))
    throw new Error(err.error || 'Failed to add tag')
  }
}

export async function removeTagFromFrame(frameId: number, tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags/${tagId}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to remove tag' }))
    throw new Error(err.error || 'Failed to remove tag')
  }
}

export async function fetchReport(params: ReportParams): Promise<ReportResponse> {
  const searchParams = new URLSearchParams()
  searchParams.set('period', params.period)
  if (params.from) searchParams.set('from', params.from)
  if (params.to) searchParams.set('to', params.to)

  const res = await fetchWithRetry(`${BASE_URL}/reports?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Report query failed' }))
    throw new Error(err.error || 'Report query failed')
  }
  return res.json()
}

export async function downloadBackup(params: BackupParams = {}): Promise<Blob> {
  const searchParams = new URLSearchParams()
  if (params.include_settings !== undefined) {
    searchParams.set('include_settings', String(params.include_settings))
  }
  if (params.include_tags !== undefined) {
    searchParams.set('include_tags', String(params.include_tags))
  }
  if (params.include_events !== undefined) {
    searchParams.set('include_events', String(params.include_events))
  }
  if (params.include_frames !== undefined) {
    searchParams.set('include_frames', String(params.include_frames))
  }

  const res = await fetchWithRetry(`${BASE_URL}/backup?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to create backup' }))
    throw new Error(err.error || 'Failed to create backup')
  }
  return res.blob()
}

export async function restoreBackup(archive: BackupArchive): Promise<RestoreResult> {
  const res = await fetchWithRetry(`${BASE_URL}/backup/restore`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(archive),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Restore failed' }))
    throw new Error(err.error || 'Restore failed')
  }
  return res.json()
}

export async function fetchTimeline(params: TimelineParams = {}): Promise<TimelineResponse> {
  const searchParams = new URLSearchParams()
  if (params.from) searchParams.set('from', params.from)
  if (params.to) searchParams.set('to', params.to)
  if (params.max_events) searchParams.set('max_events', String(params.max_events))
  if (params.max_frames) searchParams.set('max_frames', String(params.max_frames))

  const res = await fetchWithRetry(`${BASE_URL}/timeline?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Timeline query failed' }))
    throw new Error(err.error || 'Timeline query failed')
  }
  return res.json()
}

export async function fetchFocusMetrics(): Promise<FocusMetricsResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/metrics`)
  if (!res.ok) throw new Error('Focus metrics query failed')
  return res.json()
}

export async function fetchWorkSessions(from?: string, to?: string, limit = 100): Promise<WorkSession[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/focus/sessions?${params}`)
  if (!res.ok) throw new Error('Work session query failed')
  return res.json()
}

export async function fetchInterruptions(from?: string, to?: string, limit = 100): Promise<Interruption[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/focus/interruptions?${params}`)
  if (!res.ok) throw new Error('Interruption query failed')
  return res.json()
}

export async function fetchLocalSuggestions(): Promise<LocalSuggestion[]> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/suggestions`)
  if (!res.ok) throw new Error('Local suggestion query failed')
  return res.json()
}

export async function submitSuggestionFeedback(id: number, action: SuggestionFeedbackAction): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/suggestions/${id}/feedback`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action }),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Feedback submission failed' }))
    throw new Error(err.error || 'Feedback submission failed')
  }
}

export async function fetchAutomationStatus(): Promise<AutomationStatus> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/status`)
  if (!res.ok) throw new Error('Automation state query failed')
  return res.json()
}

export async function fetchAuditLogs(limit = 50, status?: string): Promise<AuditEntry[]> {
  const params = new URLSearchParams()
  params.set('limit', String(limit))
  if (status) params.set('status', status)
  const res = await fetchWithRetry(`${BASE_URL}/automation/audit?${params}`)
  if (!res.ok) throw new Error('Audit log query failed')
  return res.json()
}

export async function fetchPolicies(): Promise<PoliciesInfo> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/policies`)
  if (!res.ok) throw new Error('policy query failure')
  return res.json()
}

export async function fetchAutomationContracts(): Promise<AutomationContracts> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/contracts`)
  if (!res.ok) throw new Error('Automation contract version query failed')
  return res.json()
}

export async function fetchAutomationStats(): Promise<AutomationStats> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/stats`)
  if (!res.ok) throw new Error('Execution stats query failed')
  return res.json()
}

export async function fetchPresets(): Promise<{ presets: WorkflowPreset[] }> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets`)
  if (!res.ok) throw new Error('Preset query failed')
  return res.json()
}

export async function createPreset(preset: WorkflowPreset): Promise<WorkflowPreset> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(preset),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to create preset' }))
    throw new Error(err.error || 'Failed to create preset')
  }
  return res.json()
}

export async function deletePreset(id: string): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets/${id}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to delete preset' }))
    throw new Error(err.error || 'Failed to delete preset')
  }
}

export async function runPreset(id: string): Promise<PresetRunResult> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets/${id}/run`, {
    method: 'POST',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Preset execution failed' }))
    throw new Error(err.error || 'Preset execution failed')
  }
  return res.json()
}

export async function executeIntentHint(payload: ExecuteIntentHintRequest): Promise<ExecuteIntentHintResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/execute-hint`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Natural language intent execution failed' }))
    throw new Error(err.error || 'Natural language intent execution failed')
  }
  return res.json()
}

export async function executeSceneAction(payload: ExecuteSceneActionRequest): Promise<ExecuteSceneActionResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/execute-scene-action`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Scene action execution failed' }))
    throw new Error(err.error || 'Scene action execution failed')
  }
  return res.json()
}

export async function fetchAutomationScene(
  params: { appName?: string; screenId?: string; frameId?: number } = {},
): Promise<UiScene> {
  const query = new URLSearchParams()
  if (params.appName) query.set('app_name', params.appName)
  if (params.screenId) query.set('screen_id', params.screenId)
  if (typeof params.frameId === 'number') query.set('frame_id', String(params.frameId))

  const suffix = query.toString()
  const res = await fetchWithRetry(`${BASE_URL}/automation/scene${suffix.length > 0 ? `?${suffix}` : ''}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Scene query failure' }))
    throw new Error(err.error || 'Scene query failure')
  }
  return res.json()
}

export async function fetchSceneCalibration(
  params: { appName?: string; screenId?: string; frameId?: number } = {},
): Promise<SceneCalibrationReport> {
  const query = new URLSearchParams()
  if (params.appName) query.set('app_name', params.appName)
  if (params.screenId) query.set('screen_id', params.screenId)
  if (typeof params.frameId === 'number') query.set('frame_id', String(params.frameId))

  const suffix = query.toString()
  const res = await fetchWithRetry(`${BASE_URL}/automation/scene/calibration${suffix.length > 0 ? `?${suffix}` : ''}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Scene calibration query failure' }))
    throw new Error(err.error || 'Scene calibration query failure')
  }
  return res.json()
}
