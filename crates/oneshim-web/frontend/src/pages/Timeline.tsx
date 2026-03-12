/**
 *
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Camera } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { addTagToFrame, type Frame, fetchFrames, fetchFrameTags, fetchTags, removeTagFromFrame } from '../api/client'
import DateRangePicker from '../components/DateRangePicker'
import Lightbox from '../components/Lightbox'
import { TagBadge } from '../components/TagBadge'
import { TagInput } from '../components/TagInput'
import { Badge, Button, Card, CardTitle, EmptyState, Select, Skeleton } from '../components/ui'
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts'
import { colors, interaction, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatDate, formatTime } from '../utils/formatters'

type ViewMode = 'grid' | 'list'
type ImportanceFilter = 'all' | 'high' | 'medium' | 'low'

function getImportanceBadge(importance: number) {
  if (importance >= 0.7) return { color: 'success' as const, label: `${(importance * 100).toFixed(0)}%` }
  if (importance >= 0.4) return { color: 'warning' as const, label: `${(importance * 100).toFixed(0)}%` }
  return { color: 'default' as const, label: `${(importance * 100).toFixed(0)}%` }
}

export default function Timeline() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [selectedFrame, setSelectedFrame] = useState<Frame | null>(null)
  const [selectedIndex, setSelectedIndex] = useState<number>(-1)
  const [page, setPage] = useState(0)
  const [dateRange, setDateRange] = useState<{ from?: string; to?: string }>({})
  const [lightboxOpen, setLightboxOpen] = useState(false)
  const [viewMode, setViewMode] = useState<ViewMode>('grid')
  const [appFilter, setAppFilter] = useState<string>('all')
  const [importanceFilter, setImportanceFilter] = useState<ImportanceFilter>('all')
  const [tagFilter, setTagFilter] = useState<number | 'all'>('all')
  const pageSize = 50

  const handleRangeChange = useCallback((from: string | undefined, to: string | undefined) => {
    setDateRange({ from, to })
    setPage(0)
  }, [])

  const { data: allTags = [] } = useQuery({
    queryKey: ['tags'],
    queryFn: fetchTags,
  })

  const { data: selectedFrameTags = [] } = useQuery({
    queryKey: ['frame-tags', selectedFrame?.id],
    queryFn: () => (selectedFrame ? fetchFrameTags(selectedFrame.id) : Promise.resolve([])),
    enabled: !!selectedFrame,
  })

  const addTagMutation = useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) => addTagToFrame(frameId, tagId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['frame-tags', selectedFrame?.id] })
    },
  })

  const removeTagMutation = useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) => removeTagFromFrame(frameId, tagId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['frame-tags', selectedFrame?.id] })
    },
  })

  const { data: response, isLoading } = useQuery({
    queryKey: ['frames', page, dateRange.from, dateRange.to],
    queryFn: () => fetchFrames(dateRange.from, dateRange.to, pageSize, page * pageSize),
  })

  const frames = response?.data ?? []
  const pagination = response?.pagination

  const filteredFrames = useMemo(() => {
    return frames.filter((frame) => {
      if (appFilter !== 'all' && frame.app_name !== appFilter) return false
      if (importanceFilter === 'high' && frame.importance < 0.7) return false
      if (importanceFilter === 'medium' && (frame.importance < 0.4 || frame.importance >= 0.7)) return false
      if (importanceFilter === 'low' && frame.importance >= 0.4) return false
      if (tagFilter !== 'all' && !(frame.tag_ids ?? []).includes(tagFilter as number)) return false
      return true
    })
  }, [frames, appFilter, importanceFilter, tagFilter])

  const appList = useMemo(() => {
    const apps = new Set(frames.map((f) => f.app_name))
    return Array.from(apps).sort()
  }, [frames])

  const selectFrame = useCallback((frame: Frame, index: number) => {
    setSelectedFrame(frame)
    setSelectedIndex(index)
  }, [])

  const goToPrev = useCallback(() => {
    if (selectedIndex > 0) {
      const newIndex = selectedIndex - 1
      setSelectedFrame(filteredFrames[newIndex])
      setSelectedIndex(newIndex)
    }
  }, [selectedIndex, filteredFrames])

  const goToNext = useCallback(() => {
    if (selectedIndex < filteredFrames.length - 1) {
      const newIndex = selectedIndex + 1
      setSelectedFrame(filteredFrames[newIndex])
      setSelectedIndex(newIndex)
    }
  }, [selectedIndex, filteredFrames])

  const openLightbox = useCallback(() => {
    if (selectedFrame?.image_url) {
      setLightboxOpen(true)
    }
  }, [selectedFrame])

  useKeyboardShortcuts({
    onEscape: () => {
      if (lightboxOpen) {
        setLightboxOpen(false)
      } else {
        setSelectedFrame(null)
        setSelectedIndex(-1)
      }
    },
    onArrowLeft: goToPrev,
    onArrowRight: goToNext,
    onEnter: openLightbox,
    onSpace: openLightbox,
  })

  if (isLoading) {
    const skeletonIds = Array.from({ length: 8 }, (_, index) => `timeline-skeleton-${index}`)

    return (
      <div className="min-h-full space-y-6 p-6">
        <Skeleton className="h-8 w-40" />
        <div className="flex gap-4">
          <Skeleton className="h-10 w-40" />
          <Skeleton className="h-10 w-40" />
          <Skeleton className="h-10 w-40" />
        </div>
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
          {skeletonIds.map((skeletonId) => (
            <Skeleton key={skeletonId} className="h-40 rounded-lg" />
          ))}
        </div>
      </div>
    )
  }

  if (frames.length === 0) {
    return (
      <EmptyState
        icon={<Camera className="h-8 w-8" />}
        title={t('emptyState.timeline.title')}
        description={t('emptyState.timeline.description')}
        action={{ label: t('emptyState.timeline.action'), onClick: () => navigate('/settings') }}
      />
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <div className="flex items-center space-x-4">
          <h1 className={cn(typography.h1, colors.text.primary)}>{t('timeline.title')}</h1>
          <span className="text-content-secondary">
            {pagination ? `${pagination.total}${t('timeline.captures')}` : `${frames.length}${t('timeline.captures')}`}
            {filteredFrames.length !== frames.length && ` (${filteredFrames.length}${t('timeline.showing')})`}
          </span>
        </div>
        <DateRangePicker onRangeChange={handleRangeChange} />
      </div>

      {/* UI note */}
      <Card id="section-filters" variant="default" padding="md">
        <div className="flex flex-wrap items-center gap-4">
          {/* UI note */}
          <div id="section-by-app" className="flex items-center gap-2">
            <label htmlFor="timeline-app-filter" className="text-content-secondary text-sm">
              {t('timeline.app')}:
            </label>
            <Select
              id="timeline-app-filter"
              value={appFilter}
              onChange={(e) => setAppFilter(e.target.value)}
              selectSize="sm"
            >
              <option value="all">{t('common.all')}</option>
              {appList.map((app) => (
                <option key={app} value={app}>
                  {app}
                </option>
              ))}
            </Select>
          </div>

          {/* UI note */}
          <div id="section-by-importance" className="flex items-center gap-2">
            <label htmlFor="timeline-importance-filter" className="text-content-secondary text-sm">
              {t('timeline.importance')}:
            </label>
            <Select
              id="timeline-importance-filter"
              value={importanceFilter}
              onChange={(e) => setImportanceFilter(e.target.value as ImportanceFilter)}
              selectSize="sm"
            >
              <option value="all">{t('common.all')}</option>
              <option value="high">{t('timeline.high')}</option>
              <option value="medium">{t('timeline.medium')}</option>
              <option value="low">{t('timeline.low')}</option>
            </Select>
          </div>

          {/* UI note */}
          {allTags.length > 0 && (
            <div id="section-by-tag" className="flex items-center gap-2">
              <label htmlFor="timeline-tag-filter" className="text-content-secondary text-sm">
                {t('timeline.tag')}:
              </label>
              <Select
                id="timeline-tag-filter"
                value={tagFilter === 'all' ? 'all' : String(tagFilter)}
                onChange={(e) => setTagFilter(e.target.value === 'all' ? 'all' : Number(e.target.value))}
                selectSize="sm"
              >
                <option value="all">{t('common.all')}</option>
                {allTags.map((tag) => (
                  <option key={tag.id} value={tag.id}>
                    {tag.name}
                  </option>
                ))}
              </Select>
            </div>
          )}

          {/* UI note */}
          <div className="ml-auto flex items-center gap-1">
            <Button
              data-testid="view-grid"
              variant={viewMode === 'grid' ? 'primary' : 'secondary'}
              size="icon"
              onClick={() => setViewMode('grid')}
              title={t('timeline.gridView')}
            >
              <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"
                />
              </svg>
            </Button>
            <Button
              data-testid="view-list"
              variant={viewMode === 'list' ? 'primary' : 'secondary'}
              size="icon"
              onClick={() => setViewMode('list')}
              title={t('timeline.listView')}
            >
              <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </Button>
          </div>

          {/* UI note */}
          <div className="hidden items-center text-content-tertiary text-xs md:flex">
            <kbd className="rounded bg-hover px-1.5 py-0.5 text-content-strong">← →</kbd>
            <span className="ml-1">{t('timeline.move')}</span>
            <span className="mx-2">|</span>
            <kbd className="rounded bg-hover px-1.5 py-0.5 text-content-strong">Enter</kbd>
            <span className="ml-1">{t('timeline.enlarge')}</span>
          </div>
        </div>
      </Card>

      {/* UI note */}
      {viewMode === 'grid' && (
        <Card id="section-all" variant="default" padding="md">
          <div className="grid grid-cols-4 gap-2 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-10">
            {filteredFrames.map((frame, index) => (
              <button
                type="button"
                key={frame.id}
                data-testid={`frame-card-${frame.id}`}
                onClick={() => selectFrame(frame, index)}
                onDoubleClick={() => {
                  selectFrame(frame, index)
                  if (frame.image_url) setLightboxOpen(true)
                }}
                className={cn(
                  'aspect-video overflow-hidden rounded border-2 bg-hover transition-all hover:scale-105',
                  interaction.focusRing,
                  selectedFrame?.id === frame.id
                    ? 'border-brand-signal ring-2 ring-brand-signal/50'
                    : 'border-transparent hover:border-strong',
                )}
              >
                {frame.image_url ? (
                  <img
                    src={frame.image_url}
                    alt={frame.window_title}
                    className="h-full w-full object-cover"
                    loading="lazy"
                  />
                ) : (
                  <div className="flex h-full w-full items-center justify-center text-content-tertiary text-xs">
                    {t('timeline.noImage')}
                  </div>
                )}
              </button>
            ))}
          </div>
          {filteredFrames.length === 0 && (
            <div className="py-8 text-center text-content-secondary">
              {frames.length === 0 ? t('timeline.noFrames') : t('timeline.noFilterMatch')}
            </div>
          )}
        </Card>
      )}

      {/* UI note */}
      {viewMode === 'list' && (
        <Card variant="default" padding="none">
          <div className="divide-y divide-border-muted">
            {filteredFrames.map((frame, index) => {
              const badge = getImportanceBadge(frame.importance)
              return (
                <button
                  type="button"
                  key={frame.id}
                  data-testid={`frame-row-${frame.id}`}
                  onClick={() => selectFrame(frame, index)}
                  onDoubleClick={() => {
                    selectFrame(frame, index)
                    if (frame.image_url) setLightboxOpen(true)
                  }}
                  className={cn(
                    'flex w-full items-center gap-4 p-3 text-left transition-colors',
                    interaction.focusRing,
                    selectedFrame?.id === frame.id ? 'bg-teal-500/10' : 'hover:bg-hover/50',
                  )}
                >
                  {/* UI note */}
                  <div className="h-14 w-24 flex-shrink-0 overflow-hidden rounded bg-hover">
                    {frame.image_url ? (
                      <img
                        src={frame.image_url}
                        alt={frame.window_title}
                        className="h-full w-full object-cover"
                        loading="lazy"
                      />
                    ) : (
                      <div className="flex h-full w-full items-center justify-center text-content-tertiary text-xs">
                        {t('timeline.noImage')}
                      </div>
                    )}
                  </div>

                  {/* UI note */}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate font-medium text-content text-sm">{frame.app_name}</span>
                      <Badge color={badge.color} size="sm">
                        {badge.label}
                      </Badge>
                    </div>
                    <p className="truncate text-content-secondary text-sm">{frame.window_title}</p>
                  </div>

                  {/* UI note */}
                  <div className="flex-shrink-0 text-right text-content-tertiary text-sm">
                    <div>{formatDate(frame.timestamp)}</div>
                    <div>{formatTime(frame.timestamp)}</div>
                  </div>
                </button>
              )
            })}
          </div>
          {filteredFrames.length === 0 && (
            <div className="py-8 text-center text-content-secondary">
              {frames.length === 0 ? t('timeline.noFrames') : t('timeline.noFilterMatch')}
            </div>
          )}
        </Card>
      )}

      {/* UI note */}
      {pagination && pagination.total > pageSize && (
        <div className="flex items-center justify-center space-x-4">
          <Button variant="secondary" onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0}>
            {t('common.prev')}
          </Button>
          <span className="text-content-secondary">
            {page + 1} / {Math.ceil(pagination.total / pageSize)} {t('common.page')}
          </span>
          <Button variant="secondary" onClick={() => setPage((p) => p + 1)} disabled={!pagination.has_more}>
            {t('common.next')}
          </Button>
        </div>
      )}

      {/* UI note */}
      {selectedFrame && (
        <Card variant="default" padding="lg">
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            {/* UI note */}
            <button
              type="button"
              className="group relative aspect-video w-full cursor-pointer overflow-hidden rounded-lg bg-surface-muted"
              onClick={openLightbox}
            >
              {selectedFrame.image_url ? (
                <>
                  <img
                    src={selectedFrame.image_url}
                    alt={selectedFrame.window_title}
                    className="h-full w-full object-contain"
                  />
                  <div className="absolute inset-0 flex items-center justify-center bg-black/0 transition-colors group-hover:bg-black/30">
                    <svg
                      className="h-12 w-12 text-white opacity-0 transition-opacity group-hover:opacity-100"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                      aria-hidden="true"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v3m0 0v3m0-3h3m-3 0H7"
                      />
                    </svg>
                  </div>
                </>
              ) : (
                <div className="flex h-full w-full items-center justify-center text-content-tertiary">
                  {t('timeline.noImage')}
                </div>
              )}
            </button>

            {/* UI note */}
            <div className="space-y-4">
              <div>
                <CardTitle className="mb-2">{t('timeline.frameInfo')}</CardTitle>
                <dl className="space-y-2">
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.time')}</dt>
                    <dd className="text-content">{formatTime(selectedFrame.timestamp)}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.app')}</dt>
                    <dd className="text-content">{selectedFrame.app_name}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.windowTitle')}</dt>
                    <dd className="max-w-xs truncate text-content" title={selectedFrame.window_title}>
                      {selectedFrame.window_title}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.trigger')}</dt>
                    <dd className="text-content">{selectedFrame.trigger_type}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.importance')}</dt>
                    <dd>
                      {(() => {
                        const badge = getImportanceBadge(selectedFrame.importance)
                        return (
                          <Badge color={badge.color} size="md">
                            {badge.label}
                          </Badge>
                        )
                      })()}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-content-secondary">{t('timeline.resolution')}</dt>
                    <dd className="text-content">{selectedFrame.resolution}</dd>
                  </div>
                </dl>
              </div>

              {/* UI note */}
              <div>
                <h4 className="mb-2 font-medium text-content-secondary text-sm">{t('timeline.tags')}</h4>
                <div className="space-y-2">
                  {/* UI note */}
                  {selectedFrameTags.length > 0 && (
                    <div className="flex flex-wrap gap-1">
                      {selectedFrameTags.map((tag) => (
                        <TagBadge
                          key={tag.id}
                          name={tag.name}
                          color={tag.color}
                          size="sm"
                          onRemove={() => {
                            if (selectedFrame) {
                              removeTagMutation.mutate({ frameId: selectedFrame.id, tagId: tag.id })
                            }
                          }}
                        />
                      ))}
                    </div>
                  )}
                  {/* UI note */}
                  <TagInput
                    selectedTags={selectedFrameTags}
                    onAddTag={(tag) => {
                      if (selectedFrame) {
                        addTagMutation.mutate({ frameId: selectedFrame.id, tagId: tag.id })
                      }
                    }}
                    onRemoveTag={(tag) => {
                      if (selectedFrame) {
                        removeTagMutation.mutate({ frameId: selectedFrame.id, tagId: tag.id })
                      }
                    }}
                    placeholder={t('timeline.addTag')}
                  />
                </div>
              </div>

              {/* UI note */}
              {selectedFrame.ocr_text && (
                <div>
                  <h4 className="mb-2 font-medium text-content-secondary text-sm">{t('timeline.ocrText')}</h4>
                  <div className="max-h-32 overflow-y-auto rounded bg-surface-muted p-3 font-mono text-content-strong text-sm">
                    {selectedFrame.ocr_text}
                  </div>
                </div>
              )}

              {/* UI note */}
              <div className="flex items-center justify-between border-muted border-t pt-4">
                <Button variant="secondary" onClick={goToPrev} disabled={selectedIndex <= 0}>
                  <svg
                    className="mr-2 h-4 w-4"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    aria-hidden="true"
                  >
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                  </svg>
                  {t('common.prev')}
                </Button>
                <span className="text-content-secondary text-sm">
                  {selectedIndex + 1} / {filteredFrames.length}
                </span>
                <Button variant="secondary" onClick={goToNext} disabled={selectedIndex >= filteredFrames.length - 1}>
                  {t('common.next')}
                  <svg
                    className="ml-2 h-4 w-4"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    aria-hidden="true"
                  >
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                  </svg>
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )}

      {/* UI note */}
      {lightboxOpen && selectedFrame?.image_url && (
        <Lightbox
          imageUrl={selectedFrame.image_url}
          alt={selectedFrame.window_title}
          onClose={() => setLightboxOpen(false)}
          onPrev={selectedIndex > 0 ? goToPrev : undefined}
          onNext={selectedIndex < filteredFrames.length - 1 ? goToNext : undefined}
          hasPrev={selectedIndex > 0}
          hasNext={selectedIndex < filteredFrames.length - 1}
        />
      )}
    </div>
  )
}
