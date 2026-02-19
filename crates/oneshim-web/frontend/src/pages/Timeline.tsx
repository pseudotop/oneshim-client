/**
 * 타임라인 페이지
 *
 * 스크린샷 프레임 타임라인 + 필터링 + 상세 뷰어 + 태그
 */
import { useState, useCallback, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { Camera } from 'lucide-react'
import { fetchFrames, fetchTags, fetchFrameTags, addTagToFrame, removeTagFromFrame, Frame } from '../api/client'
import DateRangePicker from '../components/DateRangePicker'
import Lightbox from '../components/Lightbox'
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts'
import { Card, CardTitle, Button, Select, Badge, Spinner, EmptyState } from '../components/ui'
import { TagBadge } from '../components/TagBadge'
import { TagInput } from '../components/TagInput'
import { cn } from '../utils/cn'
import { formatTime, formatDate } from '../utils/formatters'

type ViewMode = 'grid' | 'list'
type ImportanceFilter = 'all' | 'high' | 'medium' | 'low'

// 중요도에 따른 배지 색상
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

  // 모든 태그 조회
  const { data: allTags = [] } = useQuery({
    queryKey: ['tags'],
    queryFn: fetchTags,
  })

  // 선택된 프레임의 태그 조회
  const { data: selectedFrameTags = [] } = useQuery({
    queryKey: ['frame-tags', selectedFrame?.id],
    queryFn: () => (selectedFrame ? fetchFrameTags(selectedFrame.id) : Promise.resolve([])),
    enabled: !!selectedFrame,
  })

  // 프레임에 태그 추가 mutation
  const addTagMutation = useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) =>
      addTagToFrame(frameId, tagId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['frame-tags', selectedFrame?.id] })
    },
  })

  // 프레임에서 태그 제거 mutation
  const removeTagMutation = useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) =>
      removeTagFromFrame(frameId, tagId),
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

  // 필터링된 프레임
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

  // 앱 목록 추출
  const appList = useMemo(() => {
    const apps = new Set(frames.map((f) => f.app_name))
    return Array.from(apps).sort()
  }, [frames])

  // 프레임 선택
  const selectFrame = useCallback((frame: Frame, index: number) => {
    setSelectedFrame(frame)
    setSelectedIndex(index)
  }, [])

  // 이전/다음 프레임 이동
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

  // 라이트박스 열기
  const openLightbox = useCallback(() => {
    if (selectedFrame?.image_url) {
      setLightboxOpen(true)
    }
  }, [selectedFrame])

  // 키보드 단축키
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
    return (
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className="text-teal-500" />
      </div>
    )
  }

  if (frames.length === 0) {
    return (
      <EmptyState
        icon={<Camera className="w-8 h-8" />}
        title={t('emptyState.timeline.title')}
        description={t('emptyState.timeline.description')}
        action={{ label: t('emptyState.timeline.action'), onClick: () => navigate('/settings') }}
      />
    )
  }

  return (
    <div className="space-y-6">
      {/* 헤더 */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center space-x-4">
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('timeline.title')}</h1>
          <span className="text-slate-600 dark:text-slate-400">
            {pagination ? `${pagination.total}${t('timeline.captures')}` : `${frames.length}${t('timeline.captures')}`}
            {filteredFrames.length !== frames.length && ` (${filteredFrames.length}${t('timeline.showing')})`}
          </span>
        </div>
        <DateRangePicker onRangeChange={handleRangeChange} />
      </div>

      {/* 필터 + 뷰 모드 */}
      <Card variant="default" padding="md">
        <div className="flex flex-wrap items-center gap-4">
          {/* 앱 필터 */}
          <div className="flex items-center gap-2">
            <label className="text-sm text-slate-600 dark:text-slate-400">{t('timeline.app')}:</label>
            <Select
              value={appFilter}
              onChange={(e) => setAppFilter(e.target.value)}
              selectSize="sm"
            >
              <option value="all">{t('common.all')}</option>
              {appList.map((app) => (
                <option key={app} value={app}>{app}</option>
              ))}
            </Select>
          </div>

          {/* 중요도 필터 */}
          <div className="flex items-center gap-2">
            <label className="text-sm text-slate-600 dark:text-slate-400">{t('timeline.importance')}:</label>
            <Select
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

          {/* 태그 필터 */}
          {allTags.length > 0 && (
            <div className="flex items-center gap-2">
              <label className="text-sm text-slate-600 dark:text-slate-400">{t('timeline.tag')}:</label>
              <Select
                value={tagFilter === 'all' ? 'all' : String(tagFilter)}
                onChange={(e) => setTagFilter(e.target.value === 'all' ? 'all' : Number(e.target.value))}
                selectSize="sm"
              >
                <option value="all">{t('common.all')}</option>
                {allTags.map((tag) => (
                  <option key={tag.id} value={tag.id}>{tag.name}</option>
                ))}
              </Select>
            </div>
          )}

          {/* 뷰 모드 토글 */}
          <div className="flex items-center gap-1 ml-auto">
            <Button
              variant={viewMode === 'grid' ? 'primary' : 'secondary'}
              size="icon"
              onClick={() => setViewMode('grid')}
              title={t('timeline.gridView')}
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
              </svg>
            </Button>
            <Button
              variant={viewMode === 'list' ? 'primary' : 'secondary'}
              size="icon"
              onClick={() => setViewMode('list')}
              title={t('timeline.listView')}
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </Button>
          </div>

          {/* 단축키 힌트 */}
          <div className="hidden md:flex items-center text-xs text-slate-500 dark:text-slate-500">
            <kbd className="px-1.5 py-0.5 bg-slate-200 dark:bg-slate-600 rounded text-slate-700 dark:text-slate-300">← →</kbd>
            <span className="ml-1">{t('timeline.move')}</span>
            <span className="mx-2">|</span>
            <kbd className="px-1.5 py-0.5 bg-slate-200 dark:bg-slate-600 rounded text-slate-700 dark:text-slate-300">Enter</kbd>
            <span className="ml-1">{t('timeline.enlarge')}</span>
          </div>
        </div>
      </Card>

      {/* 그리드 뷰 */}
      {viewMode === 'grid' && (
        <Card variant="default" padding="md">
          <div className="grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-10 gap-2">
            {filteredFrames.map((frame, index) => (
              <button
                key={frame.id}
                onClick={() => selectFrame(frame, index)}
                onDoubleClick={() => {
                  selectFrame(frame, index)
                  if (frame.image_url) setLightboxOpen(true)
                }}
                className={cn(
                  'aspect-video bg-slate-200 dark:bg-slate-700 rounded overflow-hidden border-2 transition-all hover:scale-105',
                  selectedFrame?.id === frame.id
                    ? 'border-teal-500 ring-2 ring-teal-500/50'
                    : 'border-transparent hover:border-slate-400 dark:hover:border-slate-500'
                )}
              >
                {frame.image_url ? (
                  <img
                    src={frame.image_url}
                    alt={frame.window_title}
                    className="w-full h-full object-cover"
                    loading="lazy"
                  />
                ) : (
                  <div className="w-full h-full flex items-center justify-center text-xs text-slate-500">
                    {t('timeline.noImage')}
                  </div>
                )}
              </button>
            ))}
          </div>
          {filteredFrames.length === 0 && (
            <div className="text-center py-8 text-slate-600 dark:text-slate-400">
              {frames.length === 0 ? t('timeline.noFrames') : t('timeline.noFilterMatch')}
            </div>
          )}
        </Card>
      )}

      {/* 리스트 뷰 */}
      {viewMode === 'list' && (
        <Card variant="default" padding="none">
          <div className="divide-y divide-slate-200 dark:divide-slate-700">
            {filteredFrames.map((frame, index) => {
              const badge = getImportanceBadge(frame.importance)
              return (
                <button
                  key={frame.id}
                  onClick={() => selectFrame(frame, index)}
                  onDoubleClick={() => {
                    selectFrame(frame, index)
                    if (frame.image_url) setLightboxOpen(true)
                  }}
                  className={cn(
                    'w-full flex items-center gap-4 p-3 text-left transition-colors',
                    selectedFrame?.id === frame.id
                      ? 'bg-teal-500/10'
                      : 'hover:bg-slate-200/50 dark:hover:bg-slate-700/50'
                  )}
                >
                  {/* 썸네일 */}
                  <div className="w-24 h-14 flex-shrink-0 bg-slate-200 dark:bg-slate-700 rounded overflow-hidden">
                    {frame.image_url ? (
                      <img
                        src={frame.image_url}
                        alt={frame.window_title}
                        className="w-full h-full object-cover"
                        loading="lazy"
                      />
                    ) : (
                      <div className="w-full h-full flex items-center justify-center text-xs text-slate-500">
                        {t('timeline.noImage')}
                      </div>
                    )}
                  </div>

                  {/* 정보 */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-slate-900 dark:text-white truncate">
                        {frame.app_name}
                      </span>
                      <Badge color={badge.color} size="sm">
                        {badge.label}
                      </Badge>
                    </div>
                    <p className="text-sm text-slate-600 dark:text-slate-400 truncate">
                      {frame.window_title}
                    </p>
                  </div>

                  {/* 시간 */}
                  <div className="text-right text-sm text-slate-500 dark:text-slate-500 flex-shrink-0">
                    <div>{formatDate(frame.timestamp)}</div>
                    <div>{formatTime(frame.timestamp)}</div>
                  </div>
                </button>
              )
            })}
          </div>
          {filteredFrames.length === 0 && (
            <div className="text-center py-8 text-slate-600 dark:text-slate-400">
              {frames.length === 0 ? t('timeline.noFrames') : t('timeline.noFilterMatch')}
            </div>
          )}
        </Card>
      )}

      {/* 페이지네이션 */}
      {pagination && pagination.total > pageSize && (
        <div className="flex items-center justify-center space-x-4">
          <Button
            variant="secondary"
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0}
          >
            {t('common.prev')}
          </Button>
          <span className="text-slate-600 dark:text-slate-400">
            {page + 1} / {Math.ceil(pagination.total / pageSize)} {t('common.page')}
          </span>
          <Button
            variant="secondary"
            onClick={() => setPage((p) => p + 1)}
            disabled={!pagination.has_more}
          >
            {t('common.next')}
          </Button>
        </div>
      )}

      {/* 선택된 프레임 상세 */}
      {selectedFrame && (
        <Card variant="default" padding="lg">
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* 이미지 */}
            <div
              className="aspect-video bg-slate-200 dark:bg-slate-900 rounded-lg overflow-hidden cursor-pointer group relative"
              onClick={openLightbox}
            >
              {selectedFrame.image_url ? (
                <>
                  <img
                    src={selectedFrame.image_url}
                    alt={selectedFrame.window_title}
                    className="w-full h-full object-contain"
                  />
                  <div className="absolute inset-0 flex items-center justify-center bg-black/0 group-hover:bg-black/30 transition-colors">
                    <svg className="w-12 h-12 text-white opacity-0 group-hover:opacity-100 transition-opacity" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v3m0 0v3m0-3h3m-3 0H7" />
                    </svg>
                  </div>
                </>
              ) : (
                <div className="w-full h-full flex items-center justify-center text-slate-500">
                  {t('timeline.noImage')}
                </div>
              )}
            </div>

            {/* 메타데이터 */}
            <div className="space-y-4">
              <div>
                <CardTitle className="mb-2">{t('timeline.frameInfo')}</CardTitle>
                <dl className="space-y-2">
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.time')}</dt>
                    <dd className="text-slate-900 dark:text-white">{formatTime(selectedFrame.timestamp)}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.app')}</dt>
                    <dd className="text-slate-900 dark:text-white">{selectedFrame.app_name}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.windowTitle')}</dt>
                    <dd className="text-slate-900 dark:text-white truncate max-w-xs" title={selectedFrame.window_title}>
                      {selectedFrame.window_title}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.trigger')}</dt>
                    <dd className="text-slate-900 dark:text-white">{selectedFrame.trigger_type}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.importance')}</dt>
                    <dd>
                      {(() => {
                        const badge = getImportanceBadge(selectedFrame.importance)
                        return <Badge color={badge.color} size="md">{badge.label}</Badge>
                      })()}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-slate-600 dark:text-slate-400">{t('timeline.resolution')}</dt>
                    <dd className="text-slate-900 dark:text-white">{selectedFrame.resolution}</dd>
                  </div>
                </dl>
              </div>

              {/* 태그 */}
              <div>
                <h4 className="text-sm font-medium text-slate-600 dark:text-slate-400 mb-2">{t('timeline.tags')}</h4>
                <div className="space-y-2">
                  {/* 현재 태그 표시 */}
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
                  {/* 태그 추가 */}
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

              {/* OCR 텍스트 */}
              {selectedFrame.ocr_text && (
                <div>
                  <h4 className="text-sm font-medium text-slate-600 dark:text-slate-400 mb-2">{t('timeline.ocrText')}</h4>
                  <div className="bg-slate-200 dark:bg-slate-900 rounded p-3 text-sm text-slate-700 dark:text-slate-300 max-h-32 overflow-y-auto font-mono">
                    {selectedFrame.ocr_text}
                  </div>
                </div>
              )}

              {/* 네비게이션 버튼 */}
              <div className="flex items-center justify-between pt-4 border-t border-slate-200 dark:border-slate-700">
                <Button
                  variant="secondary"
                  onClick={goToPrev}
                  disabled={selectedIndex <= 0}
                >
                  <svg className="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                  </svg>
                  {t('common.prev')}
                </Button>
                <span className="text-sm text-slate-600 dark:text-slate-400">
                  {selectedIndex + 1} / {filteredFrames.length}
                </span>
                <Button
                  variant="secondary"
                  onClick={goToNext}
                  disabled={selectedIndex >= filteredFrames.length - 1}
                >
                  {t('common.next')}
                  <svg className="w-4 h-4 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                  </svg>
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )}

      {/* 라이트박스 */}
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
