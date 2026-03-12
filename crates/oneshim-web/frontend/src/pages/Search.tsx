/**
 *
 */

import { useQuery } from '@tanstack/react-query'
import { FileText, Search as SearchIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router-dom'
import { fetchTags, type SearchResult, search } from '../api/client'
import { TagBadge } from '../components/TagBadge'
import { Badge, Button, Card, Input, Spinner } from '../components/ui'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { escapeRegex, formatDateTime } from '../utils/formatters'

function highlightText(text: string, query: string): JSX.Element {
  if (!query || !text) return <>{text}</>

  const parts = text.split(new RegExp(`(${escapeRegex(query)})`, 'gi'))
  const elements: React.ReactNode[] = []
  let offset = 0
  for (const part of parts) {
    const key = `${offset}-${part.length}`
    if (part.toLowerCase() === query.toLowerCase()) {
      elements.push(
        <mark key={key} className="rounded bg-yellow-500/30 px-0.5 text-yellow-200">
          {part}
        </mark>,
      )
    } else {
      elements.push(<span key={key}>{part}</span>)
    }
    offset += part.length
  }
  return <>{elements}</>
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

  const {
    data: response,
    isLoading,
    error,
  } = useQuery({
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
    setSelectedTagIds((prev) => (prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId]))
    setPage(0)
  }

  const handleClearTags = () => {
    setSelectedTagIds([])
    setPage(0)
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <h1 className={cn(typography.h1, colors.text.primary)}>{t('search.title')}</h1>

      {/* UI note */}
      <form id="section-recent" onSubmit={handleSearch} className="flex gap-2">
        <Input
          data-testid="search-input"
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
      <div id="section-tags" className="flex flex-wrap items-center gap-4">
        {/* UI note */}
        <div className="flex space-x-2">
          {(['all', 'frames', 'events'] as SearchType[]).map((type) => (
            <Button
              key={type}
              data-testid={`filter-${type}`}
              variant={searchType === type ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => handleTypeChange(type)}
            >
              {type === 'all' ? t('common.all') : type === 'frames' ? t('search.frames') : t('search.events')}
            </Button>
          ))}
        </div>

        {/* UI note */}
        <div className="h-8 w-px bg-hover" />

        {/* UI note */}
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-content-secondary text-sm">{t('search.filterByTags')}:</span>
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
        <div className="text-content-secondary text-sm">
          {t('search.selectedTags')}:{' '}
          {allTags
            .filter((tag) => selectedTagIds.includes(tag.id))
            .map((tag) => tag.name)
            .join(', ')}
        </div>
      )}

      {/* UI note */}
      {isLoading && (
        <div className="flex h-32 items-center justify-center">
          <Spinner size="lg" className="text-accent-teal" />
          <span className="ml-3 text-content-secondary">{t('common.loading')}</span>
        </div>
      )}

      {error && (
        <Card variant="danger" padding="md">
          <p className="text-accent-red">{t('search.searchError')}</p>
        </Card>
      )}

      {response && (
        <>
          {/* UI note */}
          <div className="text-content-secondary">
            {response.query && (
              <>
                "<span className="text-content">{response.query}</span>"{' '}
              </>
            )}
            {t('search.results')}: <span className="text-accent-teal">{response.total}</span>
            {t('search.resultCount')}
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
            <div className="py-12 text-center text-content-secondary">{t('search.noResults')}</div>
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
              <span className="text-content-secondary">
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
        <div className="py-12 text-center">
          <SearchIcon className="mx-auto mb-4 h-16 w-16 text-content-muted" />
          <div className="text-content-secondary">{t('search.enterQuery')}</div>
          <div className="mt-2 text-content-tertiary text-sm">{t('search.searchHint')}</div>
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
        <div className="h-16 w-24 flex-shrink-0 overflow-hidden rounded bg-hover">
          <img
            src={result.image_url}
            alt={result.window_title || 'Screenshot'}
            className="h-full w-full object-cover"
          />
        </div>
      )}

      {/* UI note */}
      {!isFrame && (
        <div className="flex h-16 w-16 flex-shrink-0 items-center justify-center rounded bg-hover">
          <FileText className="h-8 w-8 text-content-secondary" />
        </div>
      )}

      {/* UI note */}
      <div className="min-w-0 flex-1">
        <div className="mb-1 flex flex-wrap items-center gap-2">
          <Badge color={isFrame ? 'info' : 'primary'} size="sm">
            {isFrame ? t('search.screenshot') : t('search.event')}
          </Badge>
          <span className="text-content-secondary text-sm">{formatDateTime(result.timestamp)}</span>
          {isFrame && result.importance && (
            <span className="text-content-tertiary text-sm">
              {t('search.importance')} {(result.importance * 100).toFixed(0)}%
            </span>
          )}
        </div>

        <div className="truncate font-medium text-content">
          {result.app_name && highlightText(result.app_name, query)}
          {result.app_name && result.window_title && ' - '}
          {result.window_title && highlightText(result.window_title, query)}
        </div>

        {result.matched_text && (
          <div className="mt-1 line-clamp-2 text-content-secondary text-sm">
            {highlightText(result.matched_text, query)}
          </div>
        )}

        {/* UI note */}
        {result.tags && result.tags.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
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
