/**
 * 태그 입력 컴포넌트
 *
 * 태그 검색, 선택, 생성 기능 제공
 */
import { useState, useRef, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { fetchTags, createTag, Tag } from '../api/client'
import { TagBadge, TAG_COLORS, getRandomTagColor } from './TagBadge'
import { Input } from './ui'
import { cn } from '../utils/cn'

interface TagInputProps {
  /** 선택된 태그 목록 */
  selectedTags: Tag[]
  /** 태그 추가 핸들러 */
  onAddTag: (tag: Tag) => void
  /** 태그 제거 핸들러 */
  onRemoveTag: (tag: Tag) => void
  /** 플레이스홀더 */
  placeholder?: string
}

export function TagInput({
  selectedTags,
  onAddTag,
  onRemoveTag,
  placeholder,
}: TagInputProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [inputValue, setInputValue] = useState('')
  const [isOpen, setIsOpen] = useState(false)
  const [selectedColor, setSelectedColor] = useState(getRandomTagColor())
  const inputRef = useRef<HTMLInputElement>(null)
  const dropdownRef = useRef<HTMLDivElement>(null)

  // 모든 태그 조회
  const { data: allTags = [] } = useQuery({
    queryKey: ['tags'],
    queryFn: fetchTags,
  })

  // 태그 생성 mutation
  const createTagMutation = useMutation({
    mutationFn: createTag,
    onSuccess: (newTag) => {
      queryClient.invalidateQueries({ queryKey: ['tags'] })
      onAddTag(newTag)
      setInputValue('')
      setSelectedColor(getRandomTagColor())
    },
  })

  // 필터링된 태그 목록 (선택되지 않은 것만)
  const filteredTags = allTags.filter((tag) => {
    const notSelected = !selectedTags.some((t) => t.id === tag.id)
    const matchesSearch = tag.name.toLowerCase().includes(inputValue.toLowerCase())
    return notSelected && matchesSearch
  })

  // 입력값과 정확히 일치하는 태그 존재 여부
  const exactMatch = allTags.find(
    (tag) => tag.name.toLowerCase() === inputValue.toLowerCase()
  )
  const canCreateNew = inputValue.trim() && !exactMatch

  // 외부 클릭 시 드롭다운 닫기
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node) &&
        inputRef.current &&
        !inputRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setInputValue(e.target.value)
    if (!isOpen) setIsOpen(true)
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && canCreateNew) {
      e.preventDefault()
      createTagMutation.mutate({
        name: inputValue.trim(),
        color: selectedColor,
      })
    } else if (e.key === 'Escape') {
      setIsOpen(false)
    }
  }

  const handleSelectTag = (tag: Tag) => {
    onAddTag(tag)
    setInputValue('')
    setIsOpen(false)
    inputRef.current?.focus()
  }

  const handleCreateTag = () => {
    if (canCreateNew) {
      createTagMutation.mutate({
        name: inputValue.trim(),
        color: selectedColor,
      })
    }
  }

  return (
    <div className="relative">
      {/* 선택된 태그 목록 */}
      {selectedTags.length > 0 && (
        <div className="flex flex-wrap gap-1 mb-2">
          {selectedTags.map((tag) => (
            <TagBadge
              key={tag.id}
              name={tag.name}
              color={tag.color}
              size="sm"
              onRemove={() => onRemoveTag(tag)}
            />
          ))}
        </div>
      )}

      {/* 입력 필드 */}
      <div className="relative">
        <Input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onFocus={() => setIsOpen(true)}
          placeholder={placeholder ?? t('timeline.addTag')}
          inputSize="sm"
        />

        {/* 드롭다운 메뉴 */}
        {isOpen && (inputValue || filteredTags.length > 0) && (
          <div
            ref={dropdownRef}
            className={cn(
              'absolute z-50 mt-1 w-full rounded-lg shadow-lg',
              'bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700',
              'max-h-60 overflow-auto'
            )}
          >
            {/* 기존 태그 목록 */}
            {filteredTags.length > 0 && (
              <div className="p-1">
                {filteredTags.map((tag) => (
                  <button
                    key={tag.id}
                    type="button"
                    className={cn(
                      'w-full flex items-center gap-2 px-3 py-2 rounded-md text-left',
                      'hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors'
                    )}
                    onClick={() => handleSelectTag(tag)}
                  >
                    <span
                      className="w-3 h-3 rounded-full"
                      style={{ backgroundColor: tag.color }}
                    />
                    <span className="text-sm text-slate-900 dark:text-white">{tag.name}</span>
                  </button>
                ))}
              </div>
            )}

            {/* 새 태그 생성 옵션 */}
            {canCreateNew && (
              <>
                {filteredTags.length > 0 && (
                  <div className="border-t border-slate-200 dark:border-slate-700" />
                )}
                <div className="p-2">
                  <div className="text-xs text-slate-500 dark:text-slate-400 mb-2 px-1">
                    {t('timeline.createNewTag')}
                  </div>
                  <div className="flex items-center gap-2 mb-2">
                    <div className="flex gap-1">
                      {TAG_COLORS.slice(0, 5).map((color) => (
                        <button
                          key={color}
                          type="button"
                          className={cn(
                            'w-5 h-5 rounded-full transition-transform',
                            selectedColor === color && 'ring-2 ring-offset-1 ring-slate-400 scale-110'
                          )}
                          style={{ backgroundColor: color }}
                          onClick={() => setSelectedColor(color)}
                          aria-label={`${t('timeline.selectColor')} ${color}`}
                        />
                      ))}
                    </div>
                  </div>
                  <button
                    type="button"
                    className={cn(
                      'w-full flex items-center gap-2 px-3 py-2 rounded-md text-left',
                      'bg-teal-500/10 hover:bg-teal-500/20 transition-colors'
                    )}
                    onClick={handleCreateTag}
                    disabled={createTagMutation.isPending}
                  >
                    <span
                      className="w-3 h-3 rounded-full"
                      style={{ backgroundColor: selectedColor }}
                    />
                    <span className="text-sm text-teal-600 dark:text-teal-400">
                      "{inputValue}" {t('timeline.createTag')}
                    </span>
                    {createTagMutation.isPending && (
                      <span className="ml-auto text-xs text-slate-500">{t('timeline.creating')}</span>
                    )}
                  </button>
                </div>
              </>
            )}

            {/* 결과 없음 */}
            {filteredTags.length === 0 && !canCreateNew && (
              <div className="p-3 text-sm text-slate-500 dark:text-slate-400 text-center">
                {t('timeline.noSearchResults')}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
