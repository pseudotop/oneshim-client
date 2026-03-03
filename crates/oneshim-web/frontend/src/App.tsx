import { lazy, Suspense } from 'react'
import { Routes, Route } from 'react-router-dom'
import { TitleBar, ActivityBar, SidePanel, StatusBar, CommandPalette } from './components/shell'
import { useShellLayout } from './hooks/useShellLayout'
import { useCommandPalette } from './hooks/useCommandPalette'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
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
  const { sidebarWidth, sidebarCollapsed, toggleSidebar, onResizeStart } = useShellLayout()
  const { isOpen: isPaletteOpen, open: openPalette, close: closePalette } = useCommandPalette()

  useKeyboardShortcuts({
    onEscape: () => {
      if (isPaletteOpen) closePalette()
    },
    onToggleSidebar: toggleSidebar,
  })

  return (
    <div className="app-shell bg-white dark:bg-slate-950 text-slate-900 dark:text-white">
      <TitleBar onSearchOpen={openPalette} />

      <ActivityBar
        onToggleSidebar={toggleSidebar}
        sidebarCollapsed={sidebarCollapsed}
      />

      <SidePanel
        collapsed={sidebarCollapsed}
        width={sidebarWidth}
        onResizeStart={onResizeStart}
      />

      <main className={cn('overflow-y-auto', layout.mainContent.bg)}>
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
    </div>
  )
}

export default App
