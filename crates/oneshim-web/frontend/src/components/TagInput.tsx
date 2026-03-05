/**
 *
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { createTag, fetchTags, type Tag } from '../api/client'
import { elevation } from '../styles/tokens'
import { cn } from '../utils/cn'
import { getRandomTagColor, TAG_COLORS, TagBadge } from './TagBadge'
import { Input } from './ui'

interface TagInputProps {
  selectedTags: Tag[]
  onAddTag: (tag: Tag) => void
  onRemoveTag: (tag: Tag) => void
  placeholder?: string
}

export function TagInput({ selectedTags, onAddTag, onRemoveTag, placeholder }: TagInputProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [inputValue, setInputValue] = useState('')
  const [isOpen, setIsOpen] = useState(false)
  const [selectedColor, setSelectedColor] = useState(getRandomTagColor())
  const inputRef = useRef<HTMLInputElement>(null)
  const dropdownRef = useRef<HTMLDivElement>(null)

  const { data: allTags = [] } = useQuery({
    queryKey: ['tags'],
    queryFn: fetchTags,
  })

  const createTagMutation = useMutation({
    mutationFn: createTag,
    onSuccess: (newTag) => {
      queryClient.invalidateQueries({ queryKey: ['tags'] })
      onAddTag(newTag)
      setInputValue('')
      setSelectedColor(getRandomTagColor())
    },
  })

  const filteredTags = allTags.filter((tag) => {
    const notSelected = !selectedTags.some((t) => t.id === tag.id)
    const matchesSearch = tag.name.toLowerCase().includes(inputValue.toLowerCase())
    return notSelected && matchesSearch
  })

  const exactMatch = allTags.find((tag) => tag.name.toLowerCase() === inputValue.toLowerCase())
  const canCreateNew = inputValue.trim() && !exactMatch

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
      {/* UI note */}
      {selectedTags.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-1">
          {selectedTags.map((tag) => (
            <TagBadge key={tag.id} name={tag.name} color={tag.color} size="sm" onRemove={() => onRemoveTag(tag)} />
          ))}
        </div>
      )}

      {/* UI note */}
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

        {/* UI note */}
        {isOpen && (inputValue || filteredTags.length > 0) && (
          <div
            ref={dropdownRef}
            className={cn(
              'absolute mt-1 w-full rounded-lg',
              elevation.dropdown,
              'border border-muted bg-surface-overlay',
              'max-h-60 overflow-auto',
            )}
          >
            {/* UI note */}
            {filteredTags.length > 0 && (
              <div className="p-1">
                {filteredTags.map((tag) => (
                  <button
                    key={tag.id}
                    type="button"
                    className={cn(
                      'flex w-full items-center gap-2 rounded-md px-3 py-2 text-left',
                      'transition-colors hover:bg-hover',
                    )}
                    onClick={() => handleSelectTag(tag)}
                  >
                    <span className="h-3 w-3 rounded-full" style={{ backgroundColor: tag.color }} />
                    <span className="text-content text-sm">{tag.name}</span>
                  </button>
                ))}
              </div>
            )}

            {/* UI note */}
            {canCreateNew && (
              <>
                {filteredTags.length > 0 && <div className="border-muted border-t" />}
                <div className="p-2">
                  <div className="mb-2 px-1 text-content-secondary text-xs">{t('timeline.createNewTag')}</div>
                  <div className="mb-2 flex items-center gap-2">
                    <div className="flex gap-1">
                      {TAG_COLORS.slice(0, 5).map((color) => (
                        <button
                          key={color}
                          type="button"
                          className={cn(
                            'h-5 w-5 rounded-full transition-transform',
                            selectedColor === color && 'scale-110 ring-2 ring-slate-400 ring-offset-1',
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
                      'flex w-full items-center gap-2 rounded-md px-3 py-2 text-left',
                      'bg-teal-500/10 transition-colors hover:bg-teal-500/20',
                    )}
                    onClick={handleCreateTag}
                    disabled={createTagMutation.isPending}
                  >
                    <span className="h-3 w-3 rounded-full" style={{ backgroundColor: selectedColor }} />
                    <span className="text-accent-teal text-sm">
                      "{inputValue}" {t('timeline.createTag')}
                    </span>
                    {createTagMutation.isPending && (
                      <span className="ml-auto text-content-tertiary text-xs">{t('timeline.creating')}</span>
                    )}
                  </button>
                </div>
              </>
            )}

            {/* UI note */}
            {filteredTags.length === 0 && !canCreateNew && (
              <div className="p-3 text-center text-content-secondary text-sm">{t('timeline.noSearchResults')}</div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
