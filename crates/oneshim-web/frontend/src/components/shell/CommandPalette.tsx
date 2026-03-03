import { useState, useEffect, useRef, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import {
  LayoutDashboard, Clock, Zap, Monitor,
  Image, BarChart3, FileText,
  Settings, Info, Moon, Sun, PanelLeft, Search,
} from 'lucide-react'
import { useTheme } from '../../contexts/ThemeContext'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface PaletteItem {
  id: string
  label: string
  icon: React.ReactNode
  type: 'page' | 'action'
  action: () => void
}

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onToggleSidebar: () => void
}

export default function CommandPalette({ isOpen, onClose, onToggleSidebar }: CommandPaletteProps) {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const items = useMemo<PaletteItem[]>(() => [
    { id: 'dashboard',  label: 'Dashboard',        icon: <LayoutDashboard className="w-4 h-4" />, type: 'page',   action: () => navigate('/') },
    { id: 'timeline',   label: 'Timeline',         icon: <Clock className="w-4 h-4" />,           type: 'page',   action: () => navigate('/timeline') },
    { id: 'reports',    label: 'Reports',           icon: <BarChart3 className="w-4 h-4" />,      type: 'page',   action: () => navigate('/reports') },
    { id: 'focus',      label: 'Focus',             icon: <Image className="w-4 h-4" />,           type: 'page',   action: () => navigate('/focus') },
    { id: 'replay',     label: 'Session Replay',    icon: <Zap className="w-4 h-4" />,             type: 'page',   action: () => navigate('/replay') },
    { id: 'automation', label: 'Automation',        icon: <Monitor className="w-4 h-4" />,         type: 'page',   action: () => navigate('/automation') },
    { id: 'updates',    label: 'Updates',           icon: <FileText className="w-4 h-4" />,        type: 'page',   action: () => navigate('/updates') },
    { id: 'settings',   label: 'Settings',          icon: <Settings className="w-4 h-4" />,        type: 'page',   action: () => navigate('/settings') },
    { id: 'privacy',    label: 'Privacy',           icon: <Info className="w-4 h-4" />,             type: 'page',   action: () => navigate('/privacy') },
    { id: 'search',     label: 'Search',            icon: <Search className="w-4 h-4" />,          type: 'page',   action: () => navigate('/search') },
    { id: 'theme',      label: theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode', icon: theme === 'dark' ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />, type: 'action', action: toggleTheme },
    { id: 'sidebar',    label: 'Toggle Sidebar',    icon: <PanelLeft className="w-4 h-4" />,       type: 'action', action: onToggleSidebar },
  ], [navigate, theme, toggleTheme, onToggleSidebar])

  const filtered = useMemo(() => {
    if (!query) return items
    const q = query.toLowerCase()
    return items.filter(item => item.label.toLowerCase().includes(q))
  }, [items, query])

  useEffect(() => {
    if (isOpen) {
      setQuery('')
      setSelectedIndex(0)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [isOpen])

  useEffect(() => {
    if (selectedIndex >= filtered.length) {
      setSelectedIndex(Math.max(0, filtered.length - 1))
    }
  }, [filtered.length, selectedIndex])

  const executeItem = (item: PaletteItem) => {
    item.action()
    onClose()
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        setSelectedIndex(prev => (prev + 1) % filtered.length)
        break
      case 'ArrowUp':
        e.preventDefault()
        setSelectedIndex(prev => (prev - 1 + filtered.length) % filtered.length)
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
    <div className={cn('fixed inset-0 z-50 flex items-start justify-center pt-[15vh]', layout.commandPalette.overlay)} onClick={onClose}>
      <div
        className={cn(
          layout.commandPalette.width,
          layout.commandPalette.bg,
          layout.commandPalette.border,
          layout.commandPalette.shadow,
          'rounded-lg overflow-hidden',
        )}
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center px-4 py-3 border-b border-slate-200 dark:border-slate-700">
          <Search className="w-4 h-4 text-slate-400 mr-3 flex-shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={e => { setQuery(e.target.value); setSelectedIndex(0) }}
            onKeyDown={handleKeyDown}
            placeholder={t('commandPalette.placeholder', 'Type a command or search...')}
            className={cn('flex-1 outline-none', layout.commandPalette.input)}
          />
        </div>

        <div className="max-h-80 overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-sm text-slate-400">
              {t('commandPalette.noResults', 'No results found')}
            </div>
          ) : (
            filtered.map((item, index) => (
              <button
                key={item.id}
                onClick={() => executeItem(item)}
                onMouseEnter={() => setSelectedIndex(index)}
                className={cn(
                  'w-full flex items-center gap-3 px-4 py-2 text-left transition-colors',
                  layout.commandPalette.itemText,
                  index === selectedIndex && layout.commandPalette.itemActive,
                  layout.commandPalette.itemBg,
                )}
              >
                <span className="flex-shrink-0 text-slate-400">{item.icon}</span>
                <span className="flex-1 truncate">{item.label}</span>
                <span className={layout.commandPalette.badge}>{item.type}</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
