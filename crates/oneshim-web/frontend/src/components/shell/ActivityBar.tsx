import { useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Clock, Zap, Monitor,
  Image, BarChart3, Tag, FileText,
  Settings, Info,
} from 'lucide-react'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface NavItem {
  id: string
  to: string
  icon: React.ElementType
  label: string
  group: 'monitor' | 'data' | 'manage'
}

const navItems: NavItem[] = [
  { id: 'dashboard', to: '/',          icon: LayoutDashboard, label: 'Dashboard',  group: 'monitor' },
  { id: 'timeline',  to: '/timeline',  icon: Clock,           label: 'Timeline',   group: 'monitor' },
  { id: 'replay',    to: '/replay',    icon: Zap,             label: 'Replay',     group: 'monitor' },
  { id: 'automation',to: '/automation',icon: Monitor,         label: 'Automation', group: 'monitor' },
  { id: 'focus',     to: '/focus',     icon: Image,           label: 'Focus',      group: 'data' },
  { id: 'reports',   to: '/reports',   icon: BarChart3,       label: 'Reports',    group: 'data' },
  { id: 'search',    to: '/search',    icon: Tag,             label: 'Search',     group: 'data' },
  { id: 'updates',   to: '/updates',   icon: FileText,        label: 'Updates',    group: 'manage' },
]

const bottomItems: NavItem[] = [
  { id: 'settings', to: '/settings', icon: Settings, label: 'Settings', group: 'manage' },
  { id: 'privacy',  to: '/privacy',  icon: Info,     label: 'Privacy',  group: 'manage' },
]

interface ActivityBarProps {
  onToggleSidebar: () => void
  sidebarCollapsed: boolean
}

export default function ActivityBar({ onToggleSidebar, sidebarCollapsed }: ActivityBarProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const [tooltip, setTooltip] = useState<string | null>(null)
  const [tooltipY, setTooltipY] = useState(0)

  const isActive = (to: string) => {
    if (to === '/') return location.pathname === '/'
    return location.pathname.startsWith(to)
  }

  const handleClick = (item: NavItem) => {
    if (isActive(item.to) && !sidebarCollapsed) {
      onToggleSidebar()
    } else {
      navigate(item.to)
      if (sidebarCollapsed) onToggleSidebar()
    }
  }

  const renderItem = (item: NavItem) => {
    const Icon = item.icon
    const active = isActive(item.to)

    return (
      <button
        key={item.id}
        onClick={() => handleClick(item)}
        onMouseEnter={(e) => {
          setTooltip(item.label)
          setTooltipY(e.currentTarget.getBoundingClientRect().top)
        }}
        onMouseLeave={() => setTooltip(null)}
        className={cn(
          'relative w-full flex items-center justify-center h-11 transition-colors',
          active ? layout.activityBar.iconActive : layout.activityBar.iconDefault,
          !active && 'hover:text-slate-600 dark:hover:text-slate-300',
        )}
        title={item.label}
      >
        {active && (
          <div className={cn('absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r', layout.activityBar.indicator)} />
        )}
        <Icon className={layout.activityBar.iconSize} />
      </button>
    )
  }

  const groups = {
    monitor: navItems.filter(i => i.group === 'monitor'),
    data: navItems.filter(i => i.group === 'data'),
    manage: navItems.filter(i => i.group === 'manage'),
  }

  return (
    <div className={cn('flex flex-col items-center py-1', layout.activityBar.bg, layout.activityBar.border, layout.activityBar.width)}>
      {groups.monitor.map(renderItem)}
      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {groups.data.map(renderItem)}
      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {groups.manage.map(renderItem)}

      <div className="flex-1" />

      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />
      {bottomItems.map(renderItem)}

      {tooltip && (
        <div
          className={cn('fixed z-50 pointer-events-none', layout.activityBar.tooltip)}
          style={{ left: 56, top: tooltipY + 4 }}
        >
          {tooltip}
        </div>
      )}
    </div>
  )
}
