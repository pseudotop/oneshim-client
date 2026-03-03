import { useState, useCallback, useEffect, useRef } from 'react'
import { layout } from '../styles/tokens'

const STORAGE_KEY_WIDTH = 'oneshim-sidebar-width'
const STORAGE_KEY_COLLAPSED = 'oneshim-sidebar-collapsed'

function loadPersistedState() {
  try {
    const width = localStorage.getItem(STORAGE_KEY_WIDTH)
    const collapsed = localStorage.getItem(STORAGE_KEY_COLLAPSED)
    return {
      sidebarWidth: width ? Math.min(Math.max(Number(width), layout.sidePanel.minWidth), layout.sidePanel.maxWidth) : layout.sidePanel.defaultWidth,
      sidebarCollapsed: collapsed === 'true',
    }
  } catch {
    return { sidebarWidth: layout.sidePanel.defaultWidth, sidebarCollapsed: false }
  }
}

export function useShellLayout() {
  const persisted = loadPersistedState()
  const [sidebarWidth, setSidebarWidth] = useState(persisted.sidebarWidth)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(persisted.sidebarCollapsed)
  const [isResizing, setIsResizing] = useState(false)
  const startXRef = useRef(0)
  const startWidthRef = useRef(0)

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY_WIDTH, String(sidebarWidth))
      localStorage.setItem(STORAGE_KEY_COLLAPSED, String(sidebarCollapsed))
    } catch { /* ignore */ }
  }, [sidebarWidth, sidebarCollapsed])

  useEffect(() => {
    const width = sidebarCollapsed ? 0 : sidebarWidth
    document.documentElement.style.setProperty('--sidebar-width', `${width}px`)
  }, [sidebarWidth, sidebarCollapsed])

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed(prev => !prev)
  }, [])

  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setIsResizing(true)
    startXRef.current = e.clientX
    startWidthRef.current = sidebarWidth
  }, [sidebarWidth])

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

  return {
    sidebarWidth,
    sidebarCollapsed,
    isResizing,
    toggleSidebar,
    onResizeStart,
  }
}
