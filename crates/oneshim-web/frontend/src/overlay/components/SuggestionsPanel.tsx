import { useEffect, useState } from 'react'
import { motion, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import type { SuggestionViewDto } from '../types'
import { SuggestionHistory } from './SuggestionHistory'
import { SuggestionItem } from './SuggestionItem'
import { showToast } from './Toast'

interface SuggestionsPanelProps {
  open: boolean
  suggestions: SuggestionViewDto[]
  onClose: () => void
  onRefresh: () => Promise<void> | void
}

/**
 * Focus management: Overlay panels run in compact Tauri WebView windows.
 * - Escape key dismisses the panel (global keydown listener).
 * - No focus trap needed — the panel IS the entire window content.
 * - Interactive elements use standard tab order within the panel.
 */
export function SuggestionsPanel({ open, suggestions, onClose, onRefresh }: SuggestionsPanelProps) {
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'active' | 'history'>('active')

  useEffect(() => {
    if (!open) return

    setError(null)
    void Promise.resolve(onRefresh()).catch((e) => {
      console.warn('SuggestionsPanel refresh failed:', e)
      setError('Could not load suggestions.')
    })
  }, [open, onRefresh])

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape' && open) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  async function handleAction(id: string, action: 'accept' | 'reject' | 'defer', snoozeMinutes?: number) {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      // Tauri v2 auto-converts camelCase JS -> snake_case Rust params
      await invoke('submit_suggestion_feedback', { suggestionId: id, action, snoozeMinutes })
      setError(null)
      await Promise.resolve(onRefresh())
      showToast(
        action === 'accept' ? 'Suggestion accepted' : action === 'reject' ? 'Suggestion rejected' : 'Snoozed',
        'success',
      )
    } catch (e) {
      console.warn('Feedback failed:', e)
      setError(null)
      showToast(`Feedback failed: ${e}`, 'error')
    }
  }

  return (
    <aside
      aria-label="Suggestions panel"
      className={cn(
        'fixed top-20 right-4 z-panel max-h-[calc(100vh-10rem)] w-80 max-w-[calc(100vw-2rem)] transform rounded-xl border border-content-inverse/10 bg-surface-sunken/90 shadow-2xl backdrop-blur-md',
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

      {/* Tab bar */}
      <div className="flex border-b border-content-inverse/5">
        <button
          type="button"
          className={cn(
            'flex-1 py-2 text-xs font-medium transition-colors',
            activeTab === 'active'
              ? 'text-brand border-b-2 border-brand'
              : 'text-content-secondary hover:text-content-primary',
          )}
          onClick={() => setActiveTab('active')}
        >
          Active ({suggestions.length})
        </button>
        <button
          type="button"
          className={cn(
            'flex-1 py-2 text-xs font-medium transition-colors',
            activeTab === 'history'
              ? 'text-brand border-b-2 border-brand'
              : 'text-content-secondary hover:text-content-primary',
          )}
          onClick={() => setActiveTab('history')}
        >
          History
        </button>
      </div>

      {/* Content */}
      <div className="max-h-[calc(100vh-14rem)] overflow-y-auto">
        {error && (
          <div className="border-content-inverse/5 border-b px-4 py-2 text-semantic-error text-xs">{error}</div>
        )}
        {activeTab === 'active' ? (
          suggestions.length > 0 ? (
            <ul className="list-none">
              {suggestions.map((s) => (
                <SuggestionItem key={s.id} item={s} onAction={handleAction} />
              ))}
            </ul>
          ) : (
            <div className="px-4 py-8 text-center text-content-tertiary text-xs">No suggestions yet</div>
          )
        ) : (
          <SuggestionHistory />
        )}
      </div>
    </aside>
  )
}
