import { useMemo, useCallback, useState } from 'react'
import { useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import TreeView, { type TreeNode } from './TreeView'

interface SidebarConfig {
  titleKey: string
  nodes: { id: string; labelKey: string; children?: { id: string; labelKey: string }[] }[]
}

const pageSidebarConfig: Record<string, SidebarConfig> = {
  '/': {
    titleKey: 'nav.dashboard',
    nodes: [
      { id: 'overview', labelKey: 'sidebar.overview' },
      { id: 'metrics', labelKey: 'sidebar.systemMetrics' },
      { id: 'processes', labelKey: 'sidebar.activeProcesses' },
      { id: 'focus', labelKey: 'sidebar.focusScore' },
      { id: 'heatmap', labelKey: 'sidebar.activityHeatmap' },
      { id: 'updates', labelKey: 'sidebar.updateStatus' },
    ],
  },
  '/timeline': {
    titleKey: 'nav.timeline',
    nodes: [
      { id: 'all', labelKey: 'sidebar.allFrames' },
      { id: 'filters', labelKey: 'sidebar.filters', children: [
        { id: 'by-app', labelKey: 'sidebar.byApplication' },
        { id: 'by-tag', labelKey: 'sidebar.byTag' },
        { id: 'by-importance', labelKey: 'sidebar.byImportance' },
      ]},
    ],
  },
  '/reports': {
    titleKey: 'nav.reports',
    nodes: [
      { id: 'activity', labelKey: 'sidebar.activityReport' },
      { id: 'focus', labelKey: 'sidebar.focusReport' },
      { id: 'export', labelKey: 'sidebar.exportData' },
    ],
  },
  '/focus': {
    titleKey: 'nav.focus',
    nodes: [
      { id: 'score', labelKey: 'sidebar.currentScore' },
      { id: 'trend', labelKey: 'sidebar.weeklyTrend' },
      { id: 'sessions', labelKey: 'sidebar.focusSessions' },
      { id: 'interruptions', labelKey: 'sidebar.interruptions' },
    ],
  },
  '/replay': {
    titleKey: 'nav.replay',
    nodes: [
      { id: 'timeline', labelKey: 'sidebar.timeline' },
      { id: 'events', labelKey: 'sidebar.eventLog' },
    ],
  },
  '/automation': {
    titleKey: 'nav.automation',
    nodes: [
      { id: 'policies', labelKey: 'sidebar.policies' },
      { id: 'commands', labelKey: 'sidebar.commands' },
      { id: 'history', labelKey: 'sidebar.executionHistory' },
    ],
  },
  '/updates': {
    titleKey: 'nav.updates',
    nodes: [
      { id: 'status', labelKey: 'sidebar.currentStatus' },
      { id: 'history', labelKey: 'sidebar.updateHistory' },
    ],
  },
  '/settings': {
    titleKey: 'nav.settings',
    nodes: [
      { id: 'general', labelKey: 'sidebar.general' },
      { id: 'notification', labelKey: 'sidebar.notifications' },
      { id: 'privacy', labelKey: 'sidebar.privacy' },
      { id: 'schedule', labelKey: 'sidebar.schedule' },
      { id: 'ai', labelKey: 'sidebar.aiProvider' },
      { id: 'about', labelKey: 'sidebar.about' },
    ],
  },
  '/privacy': {
    titleKey: 'nav.privacy',
    nodes: [
      { id: 'data', labelKey: 'sidebar.dataControls' },
      { id: 'consent', labelKey: 'sidebar.consent' },
      { id: 'export', labelKey: 'sidebar.dataExport' },
    ],
  },
  '/search': {
    titleKey: 'nav.search',
    nodes: [
      { id: 'recent', labelKey: 'sidebar.recentSearches' },
      { id: 'tags', labelKey: 'sidebar.browseTags' },
    ],
  },
}

function translateNodes(
  nodes: SidebarConfig['nodes'],
  t: (key: string) => string
): TreeNode[] {
  return nodes.map(node => ({
    id: node.id,
    label: t(node.labelKey),
    children: node.children ? node.children.map(child => ({
      id: child.id,
      label: t(child.labelKey),
    })) : undefined,
  }))
}

interface SidePanelProps {
  collapsed: boolean
  width: number
  onResizeStart: (e: React.MouseEvent) => void
  onResizeByKeyboard?: (delta: number) => void
}

export default function SidePanel({ collapsed, width, onResizeStart, onResizeByKeyboard }: SidePanelProps) {
  const location = useLocation()
  const { t } = useTranslation()
  const [selectedNodeId, setSelectedNodeId] = useState<string | undefined>()

  const path = location.pathname
  const config = pageSidebarConfig[path] ?? Object.entries(pageSidebarConfig).find(
    ([key]) => key !== '/' && path.startsWith(key)
  )?.[1] ?? pageSidebarConfig['/']

  const translatedNodes = useMemo(
    () => translateNodes(config.nodes, t),
    [config.nodes, t]
  )

  const handleNodeSelect = useCallback((id: string) => {
    setSelectedNodeId(id)
    // Scroll the corresponding section into view on the main content area
    const el = document.getElementById(`section-${id}`)
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }, [])

  const handleResizeKeyDown = useCallback((e: React.KeyboardEvent) => {
    const STEP = 20
    if (e.key === 'ArrowLeft') {
      e.preventDefault()
      onResizeByKeyboard?.(-STEP)
    } else if (e.key === 'ArrowRight') {
      e.preventDefault()
      onResizeByKeyboard?.(STEP)
    }
  }, [onResizeByKeyboard])

  if (collapsed) return null

  return (
    <div className="relative flex" style={{ width }}>
      <div className={cn('flex-1 flex flex-col overflow-hidden', layout.sidePanel.bg, layout.sidePanel.border)}>
        <div className={cn('px-4 py-2 flex-shrink-0', layout.sidePanel.headerBg)}>
          <span className={layout.sidePanel.headerText}>{t(config.titleKey)}</span>
        </div>

        <div className="flex-1 overflow-y-auto px-1 py-1">
          <TreeView key={path} nodes={translatedNodes} selectedId={selectedNodeId} onSelect={handleNodeSelect} />
        </div>
      </div>

      <div
        className={cn('flex-shrink-0', layout.sidePanel.resizeHandle)}
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
