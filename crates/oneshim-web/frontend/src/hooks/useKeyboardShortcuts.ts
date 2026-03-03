import { useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'

interface ShortcutHandlers {
  onHelp?: () => void
  onEscape?: () => void
  onToggleSidebar?: () => void
  onArrowLeft?: () => void
  onArrowRight?: () => void
  onArrowUp?: () => void
  onArrowDown?: () => void
  onEnter?: () => void
  onSpace?: () => void
}

/**
 *
 */
export function useKeyboardShortcuts(handlers: ShortcutHandlers = {}, enabled = true) {
  const navigate = useNavigate()

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // Cmd+B / Ctrl+B: toggle sidebar (works even when focused in inputs)
      if ((event.metaKey || event.ctrlKey) && event.key === 'b') {
        event.preventDefault()
        handlers.onToggleSidebar?.()
        return
      }

      const target = event.target as HTMLElement
      if (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable
      ) {
        if (event.key === 'Escape' && handlers.onEscape) {
          handlers.onEscape()
          return
        }
        return
      }

      switch (event.key.toLowerCase()) {
        case 'd':
          event.preventDefault()
          navigate('/')
          break
        case 't':
          event.preventDefault()
          navigate('/timeline')
          break
        case 's':
          event.preventDefault()
          navigate('/settings')
          break
        case 'p':
          event.preventDefault()
          navigate('/privacy')
          break
        case '?':
          event.preventDefault()
          handlers.onHelp?.()
          break
        case '/':
          if (event.shiftKey) {
            event.preventDefault()
            handlers.onHelp?.()
          }
          break
      }

      switch (event.key) {
        case 'Escape':
          handlers.onEscape?.()
          break
        case 'ArrowLeft':
          event.preventDefault()
          handlers.onArrowLeft?.()
          break
        case 'ArrowRight':
          event.preventDefault()
          handlers.onArrowRight?.()
          break
        case 'ArrowUp':
          event.preventDefault()
          handlers.onArrowUp?.()
          break
        case 'ArrowDown':
          event.preventDefault()
          handlers.onArrowDown?.()
          break
        case 'Enter':
          handlers.onEnter?.()
          break
        case ' ':
          event.preventDefault()
          handlers.onSpace?.()
          break
      }
    },
    [navigate, handlers]
  )

  useEffect(() => {
    if (!enabled) return

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [enabled, handleKeyDown])
}

/**
 */
export function getShortcutsList() {
  return [
    { key: 'D', description: '대시보드로 이동' },
    { key: 'T', description: '타임라인으로 이동' },
    { key: 'S', description: '설정으로 이동' },
    { key: 'P', description: '개인정보로 이동' },
    { key: '?', description: '단축키 도움말' },
    { key: 'ESC', description: '선택 해제 / 모달 닫기' },
    { key: '← →', description: '이전/next 항목' },
    { key: 'Enter', description: '선택 확인' },
    { key: '\u2318B', description: 'Toggle Sidebar' },
    { key: '\u2318K', description: 'Command Palette' },
  ]
}
