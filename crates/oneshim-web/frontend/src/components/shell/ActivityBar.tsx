import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router-dom'
import { type RouteNode, routeTree } from '../../routes'
import { interaction, layout, motion } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { Divider } from '../ui'

const ACTIVITYBAR_WIDTH_PX = 48
const TOOLTIP_ID = 'activity-bar-tooltip'

// Derive nav items from routeTree (single source of truth).
// Stable IDs for data-testid: strip leading "/" and replace "/" with "-".
function pathToId(path: string): string {
  if (path === '/') return 'dashboard'
  if (path === '/day') return 'dashboard-day'
  return path.slice(1).replace(/\//g, '-')
}

const mainItems = routeTree.filter((r) => !r.bottom && r.icon)
const bottomItems = routeTree.filter((r) => r.bottom && r.icon)

// Static grouping — computed once outside render
const groups = {
  monitor: mainItems.filter((r) => r.group === 'monitor'),
  data: mainItems.filter((r) => r.group === 'data'),
  manage: mainItems.filter((r) => r.group === 'manage'),
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

  const isActive = useCallback(
    (node: RouteNode) => {
      if (node.path === '/') {
        // Root dashboard: match exact "/" or any of its child sub-paths
        if (location.pathname === '/') return true
        return (
          node.children?.some(
            (c) => location.pathname === `/${c.path}` || location.pathname.startsWith(`/${c.path}/`),
          ) ?? false
        )
      }
      return location.pathname === node.path || location.pathname.startsWith(`${node.path}/`)
    },
    [location.pathname],
  )

  const handleClick = useCallback(
    (node: RouteNode) => {
      if (isActive(node)) {
        if (sidebarCollapsed) {
          onToggleSidebar()
        }
        return
      }

      navigate(node.path)
      if (sidebarCollapsed) onToggleSidebar()
    },
    [isActive, sidebarCollapsed, onToggleSidebar, navigate],
  )

  const renderItem = (node: RouteNode) => {
    if (!node.icon) return null
    const Icon = node.icon
    const active = isActive(node)
    const label = t(node.labelKey)
    const id = pathToId(node.path)

    return (
      <button
        type="button"
        key={node.path}
        data-testid={`nav-${id}`}
        onClick={() => handleClick(node)}
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
          'relative flex h-11 w-full items-center justify-center',
          motion.colors,
          active ? layout.activityBar.iconActive : layout.activityBar.iconDefault,
          !active && 'hover:text-content-strong',
          interaction.focusRing,
        )}
        aria-current={active ? 'page' : undefined}
        aria-describedby={tooltip ? TOOLTIP_ID : undefined}
        aria-label={label}
        title={label}
      >
        {active && (
          <div className={cn('absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-r', layout.activityBar.indicator)} />
        )}
        <Icon className={layout.activityBar.iconSize} aria-hidden="true" />
      </button>
    )
  }

  return (
    <nav
      className={cn(
        'flex flex-col items-center py-1',
        layout.activityBar.bg,
        layout.activityBar.border,
        layout.activityBar.width,
      )}
      aria-label={t('nav.mainNavLabel', 'Main Navigation')}
    >
      {groups.monitor.map(renderItem)}
      <Divider className="my-1 w-6 border-muted" />

      {groups.data.map(renderItem)}
      <Divider className="my-1 w-6 border-muted" />

      {groups.manage.map(renderItem)}

      <div className="flex-1" />

      <Divider className="my-1 w-6 border-muted" />
      {bottomItems.map(renderItem)}

      {tooltip && (
        <div
          id={TOOLTIP_ID}
          className={cn('pointer-events-none fixed z-tooltip', layout.activityBar.tooltip)}
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
