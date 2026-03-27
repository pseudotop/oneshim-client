import { useEffect } from 'react'
import { motion, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
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
  }, [open, onRefresh])

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
      const { invoke } = await import('@tauri-apps/api/core')
      // Tauri v2 auto-converts camelCase JS -> snake_case Rust params
      await invoke('submit_suggestion_feedback', { suggestionId: id, action })
      onRefresh()
    } catch (e) {
      console.warn('Feedback failed:', e)
    }
  }

  return (
    <aside
      aria-label="Suggestions panel"
      className={cn(
        'fixed top-20 right-4 z-panel max-h-[calc(100vh-10rem)] w-80 transform rounded-xl border border-content-inverse/10 bg-surface-sunken/90 shadow-2xl backdrop-blur-md',
        motion.transform,
        open ? 'translate-x-0' : 'translate-x-[calc(100%+1rem)]',
      )}
    >
      {/* Header */}
      <div className="flex items-center justify-between border-content-inverse/5 border-b px-4 py-3">
        <span className={cn('text-content-secondary text-xs uppercase tracking-wider', typography.weight.semibold)}>
          Suggestions ({suggestions.length})
        </span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close suggestions panel"
          className={cn('text-content-tertiary text-sm hover:text-content', motion.colors)}
        >
          &times;
        </button>
      </div>

      {/* List */}
      <div className="max-h-[calc(100vh-14rem)] overflow-y-auto">
        {suggestions.length > 0 ? (
          <ul className="list-none">
            {suggestions.map((s) => (
              <SuggestionItem key={s.id} item={s} onAction={handleAction} />
            ))}
          </ul>
        ) : (
          <div className="px-4 py-8 text-center text-content-tertiary text-xs">No suggestions yet</div>
        )}
      </div>
    </aside>
  )
}
