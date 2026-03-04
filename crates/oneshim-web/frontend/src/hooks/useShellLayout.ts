import { useState, useCallback, useEffect, useRef } from 'react'
import { layout } from '../styles/tokens'

const STORAGE_KEY_WIDTH = 'oneshim-sidebar-width'
const STORAGE_KEY_COLLAPSED = 'oneshim-sidebar-collapsed'

function loadPersistedWidth(): number {
  try {
    const width = localStorage.getItem(STORAGE_KEY_WIDTH)
    return width ? Math.min(Math.max(Number(width), layout.sidePanel.minWidth), layout.sidePanel.maxWidth) : layout.sidePanel.defaultWidth
  } catch {
    return layout.sidePanel.defaultWidth
  }
}

function loadPersistedCollapsed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY_COLLAPSED) === 'true'
  } catch {
    return false
  }
}

export function useShellLayout() {
  // Lazy initializers — called only once on mount
  const [sidebarWidth, setSidebarWidth] = useState(loadPersistedWidth)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(loadPersistedCollapsed)
  const [isResizing, setIsResizing] = useState(false)
  const startXRef = useRef(0)
  const startWidthRef = useRef(0)

  // Persist to localStorage — skip during active mouse resize to avoid hundreds of writes
  useEffect(() => {
    if (isResizing) return
    try {
      localStorage.setItem(STORAGE_KEY_WIDTH, String(sidebarWidth))
      localStorage.setItem(STORAGE_KEY_COLLAPSED, String(sidebarCollapsed))
    } catch { /* ignore */ }
  }, [sidebarWidth, sidebarCollapsed, isResizing])

  useEffect(() => {
    const width = sidebarCollapsed ? 0 : sidebarWidth
    if (!Number.isFinite(width)) return
    document.documentElement.style.setProperty('--sidebar-width', `${width}px`)
  }, [sidebarWidth, sidebarCollapsed])

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed(prev => !prev)
  }, [])

  // Stable ref — does not depend on sidebarWidth (reads startWidthRef at drag time)
  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    startXRef.current = e.clientX
    // Capture current width from the DOM CSS variable (source of truth during drag)
    const raw = getComputedStyle(document.documentElement).getPropertyValue('--sidebar-width')
    startWidthRef.current = parseInt(raw, 10) || layout.sidePanel.defaultWidth
    setIsResizing(true)
  }, [])

  useEffect(() => {
    if (!isResizing) return

    const onMouseMove = (e: MouseEvent) => {
      const delta = e.clientX - startXRef.current
      const newWidth = Math.min(
        Math.max(startWidthRef.current + delta, layout.sidePanel.minWidth),
        layout.sidePanel.maxWidth
      )
      setSidebarWidth(newWidth)
    }

    const onMouseUp = () => {
      setIsResizing(false)
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isResizing])

  const onResizeByKeyboard = useCallback((delta: number) => {
    setSidebarWidth(prev => {
      const next = Math.min(Math.max(prev + delta, layout.sidePanel.minWidth), layout.sidePanel.maxWidth)
      return next
    })
  }, [])

  return {
    sidebarWidth,
    sidebarCollapsed,
    toggleSidebar,
    onResizeStart,
    onResizeByKeyboard,
  }
}
