import { useState, useCallback } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import {
  LayoutDashboard, Clock, Zap, Monitor,
  Image, BarChart3, Tag, FileText,
  Settings, Info,
} from 'lucide-react'
import { layout, interaction } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface NavItem {
  id: string
  to: string
  icon: React.ElementType
  labelKey: string
  group: 'monitor' | 'data' | 'manage'
}

const ACTIVITYBAR_WIDTH_PX = 48
const TOOLTIP_ID = 'activity-bar-tooltip'

const navItems: NavItem[] = [
  { id: 'dashboard', to: '/',          icon: LayoutDashboard, labelKey: 'nav.dashboard',  group: 'monitor' },
  { id: 'timeline',  to: '/timeline',  icon: Clock,           labelKey: 'nav.timeline',   group: 'monitor' },
  { id: 'replay',    to: '/replay',    icon: Zap,             labelKey: 'nav.replay',     group: 'monitor' },
  { id: 'automation',to: '/automation',icon: Monitor,         labelKey: 'nav.automation', group: 'monitor' },
  { id: 'focus',     to: '/focus',     icon: Image,           labelKey: 'nav.focus',      group: 'data' },
  { id: 'reports',   to: '/reports',   icon: BarChart3,       labelKey: 'nav.reports',    group: 'data' },
  { id: 'search',    to: '/search',    icon: Tag,             labelKey: 'nav.search',     group: 'data' },
  { id: 'updates',   to: '/updates',   icon: FileText,        labelKey: 'nav.updates',    group: 'manage' },
]

const bottomItems: NavItem[] = [
  { id: 'settings', to: '/settings', icon: Settings, labelKey: 'nav.settings', group: 'manage' },
  { id: 'privacy',  to: '/privacy',  icon: Info,     labelKey: 'nav.privacy',  group: 'manage' },
]

// Static grouping — computed once outside render
const groups = {
  monitor: navItems.filter(i => i.group === 'monitor'),
  data: navItems.filter(i => i.group === 'data'),
  manage: navItems.filter(i => i.group === 'manage'),
}

interface ActivityBarProps {
  onToggleSidebar: () => void
  sidebarCollapsed: boolean
}

export default function ActivityBar({ onToggleSidebar, sidebarCollapsed }: ActivityBarProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const [tooltip, setTooltip] = useState<string | null>(null)
  const [tooltipY, setTooltipY] = useState(0)

  const isActive = useCallback((to: string) => {
    if (to === '/') return location.pathname === '/'
    return location.pathname.startsWith(to)
  }, [location.pathname])

  const handleClick = useCallback((item: NavItem) => {
    if (isActive(item.to) && !sidebarCollapsed) {
      onToggleSidebar()
    } else {
      navigate(item.to)
      if (sidebarCollapsed) onToggleSidebar()
    }
  }, [isActive, sidebarCollapsed, onToggleSidebar, navigate])

  const renderItem = (item: NavItem) => {
    const Icon = item.icon
    const active = isActive(item.to)
    const label = t(item.labelKey)

    return (
      <button
        key={item.id}
        onClick={() => handleClick(item)}
        onMouseEnter={(e) => {
          setTooltip(label)
          setTooltipY(e.currentTarget.getBoundingClientRect().top)
        }}
        onMouseLeave={() => setTooltip(null)}
        onFocus={(e) => {
          setTooltip(label)
          setTooltipY(e.currentTarget.getBoundingClientRect().top)
        }}
        onBlur={() => setTooltip(null)}
        className={cn(
          'relative w-full flex items-center justify-center h-11 transition-colors',
          active ? layout.activityBar.iconActive : layout.activityBar.iconDefault,
          !active && 'hover:text-slate-600 dark:hover:text-slate-300',
          interaction.focusRing,
        )}
        aria-current={active ? 'page' : undefined}
        aria-describedby={tooltip ? TOOLTIP_ID : undefined}
        title={label}
      >
        {active && (
          <div className={cn('absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r', layout.activityBar.indicator)} />
        )}
        <Icon className={layout.activityBar.iconSize} />
      </button>
    )
  }

  return (
    <nav
      className={cn('flex flex-col items-center py-1', layout.activityBar.bg, layout.activityBar.border, layout.activityBar.width)}
      role="navigation"
      aria-label={t('nav.mainNavLabel', 'Main Navigation')}
    >
      {groups.monitor.map(renderItem)}
      <hr className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {groups.data.map(renderItem)}
      <hr className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {groups.manage.map(renderItem)}

      <div className="flex-1" />

      <hr className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />
      {bottomItems.map(renderItem)}

      {tooltip && (
        <div
          id={TOOLTIP_ID}
          className={cn('fixed z-50 pointer-events-none', layout.activityBar.tooltip)}
          style={{ left: ACTIVITYBAR_WIDTH_PX + 8, top: tooltipY + 4 }}
          role="tooltip"
        >
          {tooltip}
        </div>
      )}
    </nav>
  )
}

ActivityBar.displayName = 'ActivityBar'
