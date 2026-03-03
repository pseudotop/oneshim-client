import { useLocation } from 'react-router-dom'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import TreeView, { type TreeNode } from './TreeView'

const pageSidebarConfig: Record<string, { title: string; nodes: TreeNode[] }> = {
  '/': {
    title: 'Dashboard',
    nodes: [
      { id: 'overview', label: 'Overview' },
      { id: 'metrics', label: 'System Metrics' },
      { id: 'processes', label: 'Active Processes' },
      { id: 'focus', label: 'Focus Score' },
      { id: 'heatmap', label: 'Activity Heatmap' },
      { id: 'updates', label: 'Update Status' },
    ],
  },
  '/timeline': {
    title: 'Timeline',
    nodes: [
      { id: 'all', label: 'All Frames' },
      { id: 'filters', label: 'Filters', children: [
        { id: 'by-app', label: 'By Application' },
        { id: 'by-tag', label: 'By Tag' },
        { id: 'by-importance', label: 'By Importance' },
      ]},
    ],
  },
  '/reports': {
    title: 'Reports',
    nodes: [
      { id: 'activity', label: 'Activity Report' },
      { id: 'focus', label: 'Focus Report' },
      { id: 'export', label: 'Export Data' },
    ],
  },
  '/focus': {
    title: 'Focus',
    nodes: [
      { id: 'score', label: 'Current Score' },
      { id: 'trend', label: 'Weekly Trend' },
      { id: 'sessions', label: 'Focus Sessions' },
      { id: 'interruptions', label: 'Interruptions' },
    ],
  },
  '/replay': {
    title: 'Session Replay',
    nodes: [
      { id: 'timeline', label: 'Timeline' },
      { id: 'events', label: 'Event Log' },
    ],
  },
  '/automation': {
    title: 'Automation',
    nodes: [
      { id: 'policies', label: 'Policies' },
      { id: 'commands', label: 'Commands' },
      { id: 'history', label: 'Execution History' },
    ],
  },
  '/updates': {
    title: 'Updates',
    nodes: [
      { id: 'status', label: 'Current Status' },
      { id: 'history', label: 'Update History' },
    ],
  },
  '/settings': {
    title: 'Settings',
    nodes: [
      { id: 'general', label: 'General' },
      { id: 'notification', label: 'Notifications' },
      { id: 'privacy', label: 'Privacy' },
      { id: 'schedule', label: 'Schedule' },
      { id: 'ai', label: 'AI Provider' },
      { id: 'about', label: 'About' },
    ],
  },
  '/privacy': {
    title: 'Privacy',
    nodes: [
      { id: 'data', label: 'Data Controls' },
      { id: 'consent', label: 'Consent' },
      { id: 'export', label: 'Data Export' },
    ],
  },
  '/search': {
    title: 'Search',
    nodes: [
      { id: 'recent', label: 'Recent Searches' },
      { id: 'tags', label: 'Browse Tags' },
    ],
  },
}

interface SidePanelProps {
  collapsed: boolean
  width: number
  onResizeStart: (e: React.MouseEvent) => void
}

export default function SidePanel({ collapsed, width, onResizeStart }: SidePanelProps) {
  const location = useLocation()

  if (collapsed) return null

  const path = location.pathname
  const config = pageSidebarConfig[path] ?? Object.entries(pageSidebarConfig).find(
    ([key]) => key !== '/' && path.startsWith(key)
  )?.[1] ?? pageSidebarConfig['/']

  return (
    <div className="relative flex" style={{ width }}>
      <div className={cn('flex-1 flex flex-col overflow-hidden', layout.sidePanel.bg, layout.sidePanel.border)}>
        <div className={cn('px-4 py-2 flex-shrink-0', layout.sidePanel.headerBg)}>
          <span className={layout.sidePanel.headerText}>{config.title}</span>
        </div>

        <div className="flex-1 overflow-y-auto px-1 py-1">
          <TreeView nodes={config.nodes} />
        </div>
      </div>

      <div
        className={cn('flex-shrink-0', layout.sidePanel.resizeHandle)}
        onMouseDown={onResizeStart}
      />
    </div>
  )
}
