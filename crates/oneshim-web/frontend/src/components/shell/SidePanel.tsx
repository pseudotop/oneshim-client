import { useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router-dom'
import { routeTree } from '../../routes'
import { interaction, layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import TreeView, { type TreeNode } from './TreeView'

interface SidePanelProps {
  collapsed: boolean
  width: number
  onResizeStart: (e: React.MouseEvent) => void
  onResizeByKeyboard?: (delta: number) => void
}

export default function SidePanel({ collapsed, width, onResizeStart, onResizeByKeyboard }: SidePanelProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const { t } = useTranslation()

  const currentRoute = useMemo(() => {
    // Find the matching route node from routeTree.
    // For root path "/", match exact or any child sub-path.
    // For other paths, match by prefix.
    // Note: assumes single-level nesting (children are direct sub-paths).
    return (
      routeTree.find((r) => {
        if (r.path === '/') {
          if (location.pathname === '/') return true
          return r.children?.some(
            (c) => location.pathname === `/${c.path}` || location.pathname.startsWith(`/${c.path}/`),
          )
        }
        return location.pathname === r.path || location.pathname.startsWith(`${r.path}/`)
      }) ?? routeTree.find((r) => r.path === '/')
    )
  }, [location.pathname])

  const sidebarNodes: TreeNode[] = useMemo(() => {
    if (!currentRoute?.children) return []
    return currentRoute.children.map((child) => ({
      id: child.path,
      label: t(child.labelKey),
    }))
  }, [currentRoute, t])

  const activeChild = useMemo(() => {
    if (!currentRoute?.children) return undefined
    const segments = location.pathname.split('/')
    const lastSegment = segments[segments.length - 1]
    return currentRoute.children.find((c) => c.path === lastSegment)?.path
  }, [currentRoute, location.pathname])

  const handleNodeSelect = useCallback(
    (childPath: string) => {
      if (!currentRoute) return
      const basePath = currentRoute.path === '/' ? '' : currentRoute.path
      navigate(`${basePath}/${childPath}`)
    },
    [currentRoute, navigate],
  )

  const handleResizeKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const STEP = 20
      if (e.key === 'ArrowLeft') {
        e.preventDefault()
        onResizeByKeyboard?.(-STEP)
      } else if (e.key === 'ArrowRight') {
        e.preventDefault()
        onResizeByKeyboard?.(STEP)
      }
    },
    [onResizeByKeyboard],
  )

  if (collapsed) return null

  const titleKey = currentRoute?.labelKey ?? 'nav.dashboard'

  return (
    <div className="relative flex" style={{ width }}>
      <div className={cn('flex flex-1 flex-col overflow-hidden', layout.sidePanel.bg, layout.sidePanel.border)}>
        <div className={cn('flex-shrink-0 px-4 py-2', layout.sidePanel.headerBg)}>
          <span className={layout.sidePanel.headerText}>{t(titleKey)}</span>
        </div>

        <div className="flex-1 overflow-y-auto px-1 py-1">
          {sidebarNodes.length > 0 ? (
            <TreeView
              key={currentRoute?.path}
              nodes={sidebarNodes}
              selectedId={activeChild}
              onSelect={handleNodeSelect}
            />
          ) : null}
        </div>
      </div>

      {/* biome-ignore lint/a11y/useSemanticElements: separator role on div is intentional for resizable panel — no native <hr> equivalent for interactive vertical separator */}
      <div
        className={cn('flex-shrink-0', layout.sidePanel.resizeHandle, interaction.focusRing)}
        onMouseDown={onResizeStart}
        onKeyDown={handleResizeKeyDown}
        role="separator"
        aria-orientation="vertical"
        aria-valuenow={width}
        aria-valuemin={layout.sidePanel.minWidth}
        aria-valuemax={layout.sidePanel.maxWidth}
        tabIndex={0}
        aria-label={t('shell.resizeSidebar', 'Resize sidebar')}
      />
    </div>
  )
}

SidePanel.displayName = 'SidePanel'
