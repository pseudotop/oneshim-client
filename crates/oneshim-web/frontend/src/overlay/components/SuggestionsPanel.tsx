import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { SuggestionViewDto } from '../types'
import { SuggestionItem } from './SuggestionItem'

interface SuggestionsPanelProps {
  open: boolean
  suggestions: SuggestionViewDto[]
  onClose: () => void
  onRefresh: () => void
}

export function SuggestionsPanel({ open, suggestions, onClose, onRefresh }: SuggestionsPanelProps) {
  // Fetch when panel opens
  useEffect(() => {
    if (open) onRefresh()
  }, [open])

  // Escape key closes panel
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape' && open) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  async function handleAction(id: string, action: 'accept' | 'reject' | 'defer') {
    try {
      // Tauri v2 auto-converts camelCase JS -> snake_case Rust params
      await invoke('submit_suggestion_feedback', { suggestionId: id, action })
      onRefresh()
    } catch (e) {
      console.warn('Feedback failed:', e)
    }
  }

  return (
    <div
      className={`fixed right-4 top-20 z-[45] w-80 max-h-[calc(100vh-10rem)]
        rounded-xl border border-content-inverse/10
        bg-surface-sunken/90 shadow-2xl backdrop-blur-md
        transform transition-transform duration-300 ease-out
        ${open ? 'translate-x-0' : 'translate-x-[calc(100%+1rem)]'}
      `}
    >
      {/* Header */}
      <div className="px-4 py-3 border-b border-content-inverse/5 flex justify-between items-center">
        <span className="text-xs font-semibold uppercase tracking-wider text-content-secondary">
          Suggestions ({suggestions.length})
        </span>
        <button
          onClick={onClose}
          className="text-content-tertiary hover:text-content text-sm transition-colors"
        >
          &times;
        </button>
      </div>

      {/* List */}
      <div className="overflow-y-auto max-h-[calc(100vh-14rem)]">
        {suggestions.length > 0 ? (
          suggestions.map(s => (
            <SuggestionItem key={s.id} item={s} onAction={handleAction} />
          ))
        ) : (
          <div className="px-4 py-8 text-center text-xs text-content-tertiary">
            No suggestions yet
          </div>
        )}
      </div>
    </div>
  )
}
