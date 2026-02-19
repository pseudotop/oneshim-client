// 전역 키보드 단축키 훅
import { useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'

interface ShortcutHandlers {
  onHelp?: () => void
  onEscape?: () => void
  onArrowLeft?: () => void
  onArrowRight?: () => void
  onArrowUp?: () => void
  onArrowDown?: () => void
  onEnter?: () => void
  onSpace?: () => void
}

/**
 * 키보드 단축키 훅
 *
 * @param handlers - 각 키에 대한 핸들러 (선택적)
 * @param enabled - 단축키 활성화 여부 (기본: true)
 */
export function useKeyboardShortcuts(handlers: ShortcutHandlers = {}, enabled = true) {
  const navigate = useNavigate()

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // 입력 필드에서는 단축키 무시
      const target = event.target as HTMLElement
      if (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable
      ) {
        // ESC는 입력 필드에서도 동작
        if (event.key === 'Escape' && handlers.onEscape) {
          handlers.onEscape()
          return
        }
        return
      }

      // 전역 네비게이션 단축키
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
      }

      // 컴포넌트별 단축키
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
 * 단축키 목록 반환 (도움말 표시용)
 */
export function getShortcutsList() {
  return [
    { key: 'D', description: '대시보드로 이동' },
    { key: 'T', description: '타임라인으로 이동' },
    { key: 'S', description: '설정으로 이동' },
    { key: 'P', description: '개인정보로 이동' },
    { key: '?', description: '단축키 도움말' },
    { key: 'ESC', description: '선택 해제 / 모달 닫기' },
    { key: '← →', description: '이전/다음 항목' },
    { key: 'Enter', description: '선택 확인' },
  ]
}
