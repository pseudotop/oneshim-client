// 키보드 단축키 도움말 모달
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { getShortcutsList } from '../hooks/useKeyboardShortcuts'

interface ShortcutsHelpProps {
  onClose: () => void
}

export default function ShortcutsHelp({ onClose }: ShortcutsHelpProps) {
  const { t } = useTranslation()
  const shortcuts = getShortcutsList()

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' || event.key === '?') {
        onClose()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="bg-white dark:bg-slate-800 rounded-lg shadow-xl max-w-md w-full mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 헤더 */}
        <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            {t('shortcuts.title')}
          </h2>
          <button
            onClick={onClose}
            className="p-1 text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
            aria-label={t('timeline.close')}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* 단축키 목록 */}
        <div className="p-4 space-y-2">
          {shortcuts.map(({ key, description }) => (
            <div key={key} className="flex items-center justify-between py-1">
              <span className="text-slate-600 dark:text-slate-300">{description}</span>
              <kbd className="px-2 py-1 bg-slate-100 dark:bg-slate-700 text-slate-800 dark:text-slate-200 text-sm font-mono rounded border border-slate-300 dark:border-slate-600">
                {key}
              </kbd>
            </div>
          ))}
        </div>

        {/* 푸터 */}
        <div className="px-4 py-3 bg-slate-50 dark:bg-slate-700/50 rounded-b-lg text-center">
          <span className="text-sm text-slate-500 dark:text-slate-400">
            {t('shortcuts.closeHint')}
          </span>
        </div>
      </div>
    </div>
  )
}
