import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DevToolbar } from './components/DevToolbar'
import ErrorBoundary from './components/ErrorBoundary'
import { ActivityBar, CommandPalette, ShortcutsHelp, SidePanel, StatusBar, TitleBar } from './components/shell'
import { ToastContainer } from './components/ui'
import { ShellLayoutProvider } from './contexts/ShellLayoutContext'
import { useCommandPalette } from './hooks/useCommandPalette'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import { useShellLayout } from './hooks/useShellLayout'
import { useTauriEventBridge } from './hooks/useTauriEventBridge'
import { RouteRenderer } from './routes'
import { layout } from './styles/tokens'
import { cn } from './utils/cn'

const Onboarding = lazy(() => import('./pages/Onboarding'))

function AppShell() {
  const { t } = useTranslation()
  const { sidebarWidth, sidebarCollapsed, toggleSidebar, onResizeStart, onResizeByKeyboard } = useShellLayout()
  const { isOpen: isPaletteOpen, open: openPalette, close: closePalette, toggle: togglePalette } = useCommandPalette()
  const [isHelpOpen, setIsHelpOpen] = useState(false)
  const openHelp = useCallback(() => setIsHelpOpen(true), [])
  const closeHelp = useCallback(() => setIsHelpOpen(false), [])

  const shortcutHandlers = useMemo(
    () => ({
      onEscape: () => {
        if (isPaletteOpen) closePalette()
        else if (isHelpOpen) closeHelp()
      },
      onToggleSidebar: toggleSidebar,
      onTogglePalette: togglePalette,
      onHelp: openHelp,
    }),
    [isPaletteOpen, closePalette, isHelpOpen, closeHelp, toggleSidebar, togglePalette, openHelp],
  )

  useKeyboardShortcuts(shortcutHandlers)
  useTauriEventBridge()

  return (
    <ShellLayoutProvider sidebarCollapsed={sidebarCollapsed}>
      <div className="app-shell bg-surface-sunken text-content">
        {/* Skip navigation link for keyboard users (WCAG 2.4.1) */}
        <a
          href="#main-content"
          className="sr-only focus-visible:not-sr-only focus-visible:absolute focus-visible:top-2 focus-visible:left-2 focus-visible:z-tooltip focus-visible:rounded focus-visible:bg-brand-signal focus-visible:px-4 focus-visible:py-2 focus-visible:text-sm focus-visible:text-white"
        >
          {t('shell.skipToContent', 'Skip to main content')}
        </a>

        <TitleBar onSearchOpen={openPalette} />

        <ActivityBar onToggleSidebar={toggleSidebar} sidebarCollapsed={sidebarCollapsed} />

        <SidePanel
          collapsed={sidebarCollapsed}
          width={sidebarWidth}
          onResizeStart={onResizeStart}
          onResizeByKeyboard={onResizeByKeyboard}
        />

        <main id="main-content" className={cn('overflow-y-auto', layout.mainContent.bg)} aria-label="Main content">
          <ErrorBoundary>
            <RouteRenderer />
          </ErrorBoundary>
        </main>

        <StatusBar />

        <CommandPalette isOpen={isPaletteOpen} onClose={closePalette} onToggleSidebar={toggleSidebar} />
        {isHelpOpen && <ShortcutsHelp onClose={closeHelp} />}
        <ToastContainer />
        <DevToolbar />
      </div>
    </ShellLayoutProvider>
  )
}

function App() {
  const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null)

  useEffect(() => {
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<{ completed: boolean }>('get_onboarding_status'))
      .then((r) => setOnboardingDone(r.completed))
      .catch((e) => {
        console.debug('get_onboarding_status failed (standalone/dev mode):', e)
        setOnboardingDone(true)
      })
  }, [])

  if (onboardingDone === null) return null
  if (!onboardingDone) {
    return (
      <Suspense fallback={null}>
        <Onboarding onComplete={() => setOnboardingDone(true)} />
      </Suspense>
    )
  }

  return <AppShell />
}

export default App
