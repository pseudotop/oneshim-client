import { lazy, Suspense, useMemo, useState, useCallback } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { TitleBar, ActivityBar, SidePanel, StatusBar, CommandPalette, ShortcutsHelp } from './components/shell'
import { useShellLayout } from './hooks/useShellLayout'
import { useCommandPalette } from './hooks/useCommandPalette'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import { useTranslation } from 'react-i18next'
import { layout } from './styles/tokens'
import ErrorBoundary from './components/ErrorBoundary'
import { Spinner } from './components/ui'
import { cn } from './utils/cn'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const Timeline = lazy(() => import('./pages/Timeline'))
const Reports = lazy(() => import('./pages/Reports'))
const Focus = lazy(() => import('./pages/Focus'))
const Settings = lazy(() => import('./pages/Settings'))
const Privacy = lazy(() => import('./pages/Privacy'))
const Search = lazy(() => import('./pages/Search'))
const SessionReplay = lazy(() => import('./pages/SessionReplay'))
const Automation = lazy(() => import('./pages/Automation'))
const Updates = lazy(() => import('./pages/Updates'))

function App() {
  const { t } = useTranslation()
  const { sidebarWidth, sidebarCollapsed, toggleSidebar, onResizeStart, onResizeByKeyboard } = useShellLayout()
  const { isOpen: isPaletteOpen, open: openPalette, close: closePalette, toggle: togglePalette } = useCommandPalette()
  const [isHelpOpen, setIsHelpOpen] = useState(false)
  const openHelp = useCallback(() => setIsHelpOpen(true), [])
  const closeHelp = useCallback(() => setIsHelpOpen(false), [])

  const shortcutHandlers = useMemo(() => ({
    onEscape: () => {
      if (isPaletteOpen) closePalette()
      else if (isHelpOpen) closeHelp()
    },
    onToggleSidebar: toggleSidebar,
    onTogglePalette: togglePalette,
    onHelp: openHelp,
  }), [isPaletteOpen, closePalette, isHelpOpen, closeHelp, toggleSidebar, togglePalette, openHelp])

  useKeyboardShortcuts(shortcutHandlers)

  return (
    <div className="app-shell bg-white dark:bg-slate-950 text-slate-900 dark:text-white">
      {/* Skip navigation link for keyboard users (WCAG 2.4.1) */}
      <a href="#main-content" className="sr-only focus:not-sr-only focus:absolute focus:z-[60] focus:top-2 focus:left-2 focus:px-4 focus:py-2 focus:bg-teal-600 focus:text-white focus:rounded focus:text-sm">
        {t('shell.skipToContent', 'Skip to main content')}
      </a>

      <TitleBar onSearchOpen={openPalette} />

      <ActivityBar
        onToggleSidebar={toggleSidebar}
        sidebarCollapsed={sidebarCollapsed}
      />

      <SidePanel
        collapsed={sidebarCollapsed}
        width={sidebarWidth}
        onResizeStart={onResizeStart}
        onResizeByKeyboard={onResizeByKeyboard}
      />

      <main id="main-content" className={cn('overflow-y-auto', layout.mainContent.bg)} aria-label="Main content">
        <ErrorBoundary>
          <Suspense
            fallback={
              <div className="flex items-center justify-center h-full">
                <Spinner size="lg" />
              </div>
            }
          >
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/timeline" element={<Timeline />} />
              <Route path="/reports" element={<Reports />} />
              <Route path="/focus" element={<Focus />} />
              <Route path="/replay" element={<SessionReplay />} />
              <Route path="/automation" element={<Automation />} />
              <Route path="/updates" element={<Updates />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/privacy" element={<Privacy />} />
              <Route path="/search" element={<Search />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </Suspense>
        </ErrorBoundary>
      </main>

      <StatusBar />

      <CommandPalette
        isOpen={isPaletteOpen}
        onClose={closePalette}
        onToggleSidebar={toggleSidebar}
      />
      {isHelpOpen && <ShortcutsHelp onClose={closeHelp} />}
    </div>
  )
}

export default App
