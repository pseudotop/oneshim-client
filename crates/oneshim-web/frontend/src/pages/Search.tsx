/**
 *
 */
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Search as SearchIcon, FileText } from 'lucide-react'
import { search, SearchResult, fetchTags } from '../api/client'
import { TagBadge } from '../components/TagBadge'
import { Button, Input, Card, Spinner, Badge } from '../components/ui'
import { formatDateTime, escapeRegex } from '../utils/formatters'

function highlightText(text: string, query: string): JSX.Element {
  if (!query || !text) return <>{text}</>

  const parts = text.split(new RegExp(`(${escapeRegex(query)})`, 'gi'))
  return (
    <>
      {parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark key={i} className="bg-yellow-500/30 text-yellow-200 rounded px-0.5">
            {part}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  )
}

type SearchType = 'all' | 'frames' | 'events'

export default function Search() {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const initialQuery = searchParams.get('q') || ''
  const initialTagIds = searchParams.get('tags')?.split(',').map(Number).filter(Boolean) || []

  const [inputValue, setInputValue] = useState(initialQuery)
  const [searchQuery, setSearchQuery] = useState(initialQuery)
  const [searchType, setSearchType] = useState<SearchType>('all')
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>(initialTagIds)
  const [page, setPage] = useState(0)
  const pageSize = 20

  const { data: allTags = [] } = useQuery({
    queryKey: ['tags'],
    queryFn: fetchTags,
  })

  const hasSearchCriteria = searchQuery.length > 0 || selectedTagIds.length > 0

  const { data: response, isLoading, error } = useQuery({
    queryKey: ['search', searchQuery, searchType, selectedTagIds, page],
    queryFn: () =>
      search({
        query: searchQuery,
        searchType,
        tagIds: selectedTagIds.length > 0 ? selectedTagIds : undefined,
        limit: pageSize,
        offset: page * pageSize,
      }),
    enabled: hasSearchCriteria,
  })

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = inputValue.trim()
    if (trimmed || selectedTagIds.length > 0) {
      setSearchQuery(trimmed)
      const params: Record<string, string> = {}
      if (trimmed) params.q = trimmed
      if (selectedTagIds.length > 0) params.tags = selectedTagIds.join(',')
      setSearchParams(params)
      setPage(0)
    }
  }

  const handleTypeChange = (type: SearchType) => {
    setSearchType(type)
    setPage(0)
  }

  const handleTagToggle = (tagId: number) => {
    setSelectedTagIds((prev) =>
      prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId]
    )
    setPage(0)
  }

  const handleClearTags = () => {
    setSelectedTagIds([])
    setPage(0)
  }

  return (
    <div className="h-full overflow-y-auto p-6 space-y-6">
      {/* UI note */}
      <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('search.title')}</h1>

      {/* UI note */}
      <form onSubmit={handleSearch} className="flex gap-2">
        <Input
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          placeholder={t('search.placeholder')}
          className="flex-1"
        />
        <Button type="submit" variant="primary" size="lg">
          {t('common.search')}
        </Button>
      </form>

      {/* UI note */}
      <div className="flex flex-wrap items-center gap-4">
        {/* UI note */}
        <div className="flex space-x-2">
          {(['all', 'frames', 'events'] as SearchType[]).map((type) => (
            <Button
              key={type}
              variant={searchType === type ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => handleTypeChange(type)}
            >
              {type === 'all' ? t('common.all') : type === 'frames' ? t('search.frames') : t('search.events')}
            </Button>
          ))}
        </div>

        {/* UI note */}
        <div className="w-px h-8 bg-slate-300 dark:bg-slate-700" />

        {/* UI note */}
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-slate-600 dark:text-slate-400">{t('search.filterByTags')}:</span>
          {allTags.map((tag) => (
            <TagBadge
              key={tag.id}
              name={tag.name}
              color={tag.color}
              size="sm"
              selected={selectedTagIds.includes(tag.id)}
              onClick={() => handleTagToggle(tag.id)}
            />
          ))}
          {selectedTagIds.length > 0 && (
            <Button variant="ghost" size="sm" onClick={handleClearTags}>
              {t('search.clearTags')}
            </Button>
          )}
        </div>
      </div>

      {/* UI note */}
      {selectedTagIds.length > 0 && (
        <div className="text-sm text-slate-600 dark:text-slate-400">
          {t('search.selectedTags')}:{' '}
          {allTags
            .filter((tag) => selectedTagIds.includes(tag.id))
            .map((tag) => tag.name)
            .join(', ')}
        </div>
      )}

      {/* UI note */}
      {isLoading && (
        <div className="flex items-center justify-center h-32">
          <Spinner size="lg" className="text-teal-500" />
          <span className="ml-3 text-slate-600 dark:text-slate-400">{t('common.loading')}</span>
        </div>
      )}

      {error && (
        <Card variant="danger" padding="md">
          <p className="text-red-600 dark:text-red-400">{t('search.searchError')}</p>
        </Card>
      )}

      {response && (
        <>
          {/* UI note */}
          <div className="text-slate-600 dark:text-slate-400">
            {response.query && (
              <>
                "<span className="text-slate-900 dark:text-white">{response.query}</span>"{' '}
              </>
            )}
            {t('search.results')}:{' '}
            <span className="text-teal-600 dark:text-teal-400">{response.total}</span>{t('search.resultCount')}
          </div>

          {/* UI note */}
          {response.results.length > 0 ? (
            <div className="space-y-3">
              {response.results.map((result) => (
                <SearchResultCard
                  key={`${result.result_type}-${result.id}`}
                  result={result}
                  query={searchQuery}
                  onTagClick={handleTagToggle}
                  selectedTagIds={selectedTagIds}
                />
              ))}
            </div>
          ) : (
            <div className="text-center py-12 text-slate-600 dark:text-slate-400">
              {t('search.noResults')}
            </div>
          )}

          {/* UI note */}
          {response.total > pageSize && (
            <div className="flex items-center justify-center space-x-4">
              <Button
                variant="secondary"
                size="md"
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={page === 0}
              >
                {t('common.prev')}
              </Button>
              <span className="text-slate-600 dark:text-slate-400">
                {page + 1} / {Math.ceil(response.total / pageSize)} {t('common.page')}
              </span>
              <Button
                variant="secondary"
                size="md"
                onClick={() => setPage((p) => p + 1)}
                disabled={(page + 1) * pageSize >= response.total}
              >
                {t('common.next')}
              </Button>
            </div>
          )}
        </>
      )}

      {/* UI note */}
      {!hasSearchCriteria && (
        <div className="text-center py-12">
          <SearchIcon className="w-16 h-16 mx-auto mb-4 text-slate-400 dark:text-slate-500" />
          <div className="text-slate-600 dark:text-slate-400">{t('search.enterQuery')}</div>
          <div className="text-sm text-slate-500 mt-2">
            {t('search.searchHint')}
          </div>
        </div>
      )}
    </div>
  )
}

interface SearchResultCardProps {
  result: SearchResult
  query: string
  onTagClick: (tagId: number) => void
  selectedTagIds: number[]
}

function SearchResultCard({ result, query, onTagClick, selectedTagIds }: SearchResultCardProps) {
  const { t } = useTranslation()
  const isFrame = result.result_type === 'frame'

  return (
    <Card padding="md" className="flex gap-4">
      {/* UI note */}
      {isFrame && result.image_url && (
        <div className="w-24 h-16 flex-shrink-0 bg-slate-200 dark:bg-slate-700 rounded overflow-hidden">
          <img
            src={result.image_url}
            alt={result.window_title || 'Screenshot'}
            className="w-full h-full object-cover"
          />
        </div>
      )}

      {/* UI note */}
      {!isFrame && (
        <div className="w-16 h-16 flex-shrink-0 bg-slate-200 dark:bg-slate-700 rounded flex items-center justify-center">
          <FileText className="w-8 h-8 text-slate-500 dark:text-slate-400" />
        </div>
      )}

      {/* UI note */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1 flex-wrap">
          <Badge color={isFrame ? 'info' : 'primary'} size="sm">
            {isFrame ? t('search.screenshot') : t('search.event')}
          </Badge>
          <span className="text-sm text-slate-500 dark:text-slate-400">{formatDateTime(result.timestamp)}</span>
          {isFrame && result.importance && (
            <span className="text-sm text-slate-500">
              {t('search.importance')} {(result.importance * 100).toFixed(0)}%
            </span>
          )}
        </div>

        <div className="text-slate-900 dark:text-white font-medium truncate">
          {result.app_name && highlightText(result.app_name, query)}
          {result.app_name && result.window_title && ' - '}
          {result.window_title && highlightText(result.window_title, query)}
        </div>

        {result.matched_text && (
          <div className="text-sm text-slate-600 dark:text-slate-400 mt-1 line-clamp-2">
            {highlightText(result.matched_text, query)}
          </div>
        )}

        {/* UI note */}
        {result.tags && result.tags.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-2">
            {result.tags.map((tag) => (
              <TagBadge
                key={tag.id}
                name={tag.name}
                color={tag.color}
                size="sm"
                selected={selectedTagIds.includes(tag.id)}
                onClick={() => onTagClick(tag.id)}
              />
            ))}
          </div>
        )}
      </div>
    </Card>
  )
}
