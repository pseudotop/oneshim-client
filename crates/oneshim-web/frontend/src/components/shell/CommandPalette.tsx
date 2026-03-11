import {
  BarChart3,
  Clock,
  FileText,
  Image,
  Info,
  LayoutDashboard,
  Monitor,
  Moon,
  PanelLeft,
  Search,
  Settings,
  Sun,
  Zap,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { useTheme } from '../../contexts/ThemeContext'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface PaletteItem {
  id: string
  labelKey: string
  labelFallback: string
  icon: React.ReactNode
  type: 'page' | 'action'
  action: () => void
}

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onToggleSidebar: () => void
}

const LISTBOX_ID = 'command-palette-listbox'

export default function CommandPalette({ isOpen, onClose, onToggleSidebar }: CommandPaletteProps) {
  const navigate = useNavigate()
  const navigateRef = useRef(navigate)
  navigateRef.current = navigate
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const dialogRef = useRef<HTMLDivElement>(null)

  const items = useMemo<PaletteItem[]>(
    () => [
      {
        id: 'dashboard',
        labelKey: 'nav.dashboard',
        labelFallback: 'Dashboard',
        icon: <LayoutDashboard className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/'),
      },
      {
        id: 'timeline',
        labelKey: 'nav.timeline',
        labelFallback: 'Timeline',
        icon: <Clock className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/timeline'),
      },
      {
        id: 'reports',
        labelKey: 'nav.reports',
        labelFallback: 'Reports',
        icon: <BarChart3 className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/reports'),
      },
      {
        id: 'focus',
        labelKey: 'nav.focus',
        labelFallback: 'Focus',
        icon: <Image className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/focus'),
      },
      {
        id: 'replay',
        labelKey: 'nav.replay',
        labelFallback: 'Session Replay',
        icon: <Zap className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/replay'),
      },
      {
        id: 'automation',
        labelKey: 'nav.automation',
        labelFallback: 'Automation',
        icon: <Monitor className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/automation'),
      },
      {
        id: 'updates',
        labelKey: 'nav.updates',
        labelFallback: 'Updates',
        icon: <FileText className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/updates'),
      },
      {
        id: 'settings',
        labelKey: 'nav.settings',
        labelFallback: 'Settings',
        icon: <Settings className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/settings'),
      },
      {
        id: 'privacy',
        labelKey: 'nav.privacy',
        labelFallback: 'Privacy',
        icon: <Info className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/privacy'),
      },
      {
        id: 'search',
        labelKey: 'nav.search',
        labelFallback: 'Search',
        icon: <Search className="h-4 w-4" aria-hidden="true" />,
        type: 'page',
        action: () => navigateRef.current('/search'),
      },
      {
        id: 'theme',
        labelKey: theme === 'dark' ? 'shell.switchToLight' : 'shell.switchToDark',
        labelFallback: theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode',
        icon:
          theme === 'dark' ? (
            <Sun className="h-4 w-4" aria-hidden="true" />
          ) : (
            <Moon className="h-4 w-4" aria-hidden="true" />
          ),
        type: 'action',
        action: toggleTheme,
      },
      {
        id: 'sidebar',
        labelKey: 'shell.toggleSidebar',
        labelFallback: 'Toggle Sidebar',
        icon: <PanelLeft className="h-4 w-4" aria-hidden="true" />,
        type: 'action',
        action: onToggleSidebar,
      },
    ],
    [theme, toggleTheme, onToggleSidebar],
  )

  const getLabel = useCallback(
    (item: PaletteItem) => {
      if (item.id === 'theme') {
        return theme === 'dark'
          ? t('shell.switchToLight', 'Switch to Light Mode')
          : t('shell.switchToDark', 'Switch to Dark Mode')
      }
      return item.labelKey ? t(item.labelKey, item.labelFallback) : item.labelFallback
    },
    [t, theme],
  )

  const filtered = useMemo(() => {
    if (!query) return items
    const q = query.toLowerCase()
    return items.filter((item) => getLabel(item).toLowerCase().includes(q))
  }, [items, query, getLabel])

  const activeDescendant = filtered[selectedIndex] ? `palette-option-${filtered[selectedIndex].id}` : undefined

  useEffect(() => {
    if (isOpen) {
      setQuery('')
      setSelectedIndex(0)
      const timer = setTimeout(() => inputRef.current?.focus(), 50)
      return () => clearTimeout(timer)
    }
  }, [isOpen])

  useEffect(() => {
    if (selectedIndex >= filtered.length) {
      setSelectedIndex(Math.max(0, filtered.length - 1))
    }
  }, [filtered.length, selectedIndex])

  // Focus trap: keep focus within the dialog
  useEffect(() => {
    if (!isOpen) return

    const handleFocusTrap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab' || !dialogRef.current) return

      const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
        'input, button, [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return

      const first = focusable[0]
      const last = focusable[focusable.length - 1]

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }

    document.addEventListener('keydown', handleFocusTrap)
    return () => document.removeEventListener('keydown', handleFocusTrap)
  }, [isOpen])

  const executeItem = (item: PaletteItem) => {
    item.action()
    onClose()
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        setSelectedIndex((prev) => (prev + 1) % filtered.length)
        break
      case 'ArrowUp':
        e.preventDefault()
        setSelectedIndex((prev) => (prev - 1 + filtered.length) % filtered.length)
        break
      case 'Enter':
        e.preventDefault()
        if (filtered[selectedIndex]) executeItem(filtered[selectedIndex])
        break
      case 'Escape':
        e.preventDefault()
        onClose()
        break
    }
  }

  if (!isOpen) return null

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop overlay — Escape handled in input onKeyDown
    // biome-ignore lint/a11y/useKeyWithClickEvents: Escape key handled via input onKeyDown handler
    <div
      className={cn('fixed inset-0 z-50 flex items-start justify-center pt-[15vh]', layout.commandPalette.overlay)}
      onClick={onClose}
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: onClick only prevents bubble to backdrop, not interactive */}
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        data-testid="command-palette"
        aria-label={t('commandPalette.dialogLabel', 'Command Palette')}
        className={cn(
          layout.commandPalette.width,
          layout.commandPalette.bg,
          layout.commandPalette.border,
          layout.commandPalette.shadow,
          'overflow-hidden rounded-lg',
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center border-muted border-b px-4 py-3">
          <Search className="mr-3 h-4 w-4 flex-shrink-0 text-content-muted" aria-hidden="true" />
          <input
            ref={inputRef}
            type="text"
            data-testid="command-palette-input"
            role="combobox"
            aria-expanded={true}
            aria-haspopup="listbox"
            aria-controls={LISTBOX_ID}
            aria-activedescendant={activeDescendant}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setSelectedIndex(0)
            }}
            onKeyDown={handleKeyDown}
            placeholder={t('commandPalette.placeholder', 'Type a command or search...')}
            aria-label={t('commandPalette.placeholder', 'Type a command or search...')}
            className={cn('flex-1 outline-none', layout.commandPalette.input)}
          />
        </div>

        <div className="max-h-80 overflow-y-auto py-1" role="listbox" id={LISTBOX_ID}>
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-content-muted text-sm">
              {t('commandPalette.noResults', 'No results found')}
            </div>
          ) : (
            filtered.map((item, index) => (
              // biome-ignore lint/a11y/useKeyWithClickEvents: combobox APG — keyboard handled at input level (ArrowDown/Up/Enter)
              <div
                key={item.id}
                id={`palette-option-${item.id}`}
                role="option"
                aria-selected={index === selectedIndex}
                tabIndex={-1}
                onClick={() => executeItem(item)}
                onMouseEnter={() => setSelectedIndex(index)}
                className={cn(
                  'flex w-full cursor-pointer items-center gap-3 px-4 py-2 text-left transition-colors',
                  layout.commandPalette.itemText,
                  index === selectedIndex && layout.commandPalette.itemActive,
                  layout.commandPalette.itemBg,
                )}
              >
                <span className="flex-shrink-0 text-content-muted">{item.icon}</span>
                <span className="flex-1 truncate">{getLabel(item)}</span>
                <span className={layout.commandPalette.badge}>{item.type}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}

CommandPalette.displayName = 'CommandPalette'
