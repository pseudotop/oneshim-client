import { useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { useCurrentRoute } from '../../routes'
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
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { node: currentRoute, child: activeChild } = useCurrentRoute()

  const sidebarNodes: TreeNode[] = useMemo(() => {
    if (!currentRoute.children) return []
    return currentRoute.children.map((child) => ({
      id: child.path,
      label: t(child.labelKey),
    }))
  }, [currentRoute, t])

  const handleNodeSelect = useCallback(
    (childPath: string) => {
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

  // Collapsed OR the current route has no children → hide the sidebar entirely.
  // Routes like /day, /chat, /search, /playbooks, /policies have no sub-nav and
  // an empty panel would just steal horizontal space from the main content.
  if (collapsed || sidebarNodes.length === 0) return null

  return (
    <div className="relative flex" style={{ width }}>
      <div className={cn('flex flex-1 flex-col overflow-hidden', layout.sidePanel.bg, layout.sidePanel.border)}>
        <div className={cn('flex-shrink-0 px-4 py-2', layout.sidePanel.headerBg)}>
          <span className={layout.sidePanel.headerText}>{t(currentRoute.labelKey)}</span>
        </div>

        <div className="flex-1 overflow-y-auto px-1 py-1">
          <TreeView
            key={currentRoute.path}
            nodes={sidebarNodes}
            selectedId={activeChild?.path}
            onSelect={handleNodeSelect}
          />
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
