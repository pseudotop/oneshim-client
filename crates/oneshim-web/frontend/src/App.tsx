import { useState, useEffect, useRef, lazy, Suspense } from 'react'
import { Routes, Route, NavLink, useNavigate, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useTheme } from './contexts/ThemeContext'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import ErrorBoundary from './components/ErrorBoundary'
import ShortcutsHelp from './components/ShortcutsHelp'
import LanguageSelector from './components/LanguageSelector'
import { Spinner } from './components/ui'

// Lazy load page components for code splitting
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

type NavItem = {
  to: string
  labelKey: string
  mobileLabel: string
}

const primaryNavItems: NavItem[] = [
  { to: '/', labelKey: 'nav.dashboard', mobileLabel: 'D' },
  { to: '/timeline', labelKey: 'nav.timeline', mobileLabel: 'T' },
  { to: '/reports', labelKey: 'nav.reports', mobileLabel: 'R' },
  { to: '/focus', labelKey: 'nav.focus', mobileLabel: 'F' },
]

const secondaryNavItems: NavItem[] = [
  { to: '/replay', labelKey: 'nav.replay', mobileLabel: 'V' },
  { to: '/automation', labelKey: 'nav.automation', mobileLabel: 'A' },
  { to: '/updates', labelKey: 'nav.updates', mobileLabel: 'U' },
  { to: '/settings', labelKey: 'nav.settings', mobileLabel: 'S' },
  { to: '/privacy', labelKey: 'nav.privacy', mobileLabel: 'P' },
  { to: '/search', labelKey: 'nav.search', mobileLabel: '🔍' },
]

function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [searchInput, setSearchInput] = useState('')
  const [showShortcutsHelp, setShowShortcutsHelp] = useState(false)
  const [showMoreMenu, setShowMoreMenu] = useState(false)
  const moreMenuRef = useRef<HTMLDivElement | null>(null)

  const isSecondaryActive = secondaryNavItems.some((item) => item.to === location.pathname)

  useEffect(() => {
    const onOutsideClick = (event: MouseEvent) => {
      if (!moreMenuRef.current) {
        return
      }
      if (!moreMenuRef.current.contains(event.target as Node)) {
        setShowMoreMenu(false)
      }
    }

    const onEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setShowMoreMenu(false)
      }
    }

    window.addEventListener('click', onOutsideClick)
    window.addEventListener('keydown', onEscape)
    return () => {
      window.removeEventListener('click', onOutsideClick)
      window.removeEventListener('keydown', onEscape)
    }
  }, [])

  // 전역 키보드 단축키
  useKeyboardShortcuts({
    onHelp: () => setShowShortcutsHelp(true),
    onEscape: () => setShowShortcutsHelp(false),
  })

  const handleGlobalSearch = (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = searchInput.trim()
    if (trimmed) {
      navigate(`/search?q=${encodeURIComponent(trimmed)}`)
      setSearchInput('')
    }
  }

  return (
    <div className="min-h-screen bg-white dark:bg-slate-900 transition-colors">
      {/* 네비게이션 */}
      <nav className="bg-slate-100 dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700 transition-colors">
        <div className="max-w-7xl mx-auto px-4">
          <div className="flex items-center justify-between h-14">
            <div className="flex items-center space-x-8">
              <span className="text-xl font-bold text-teal-600 dark:text-teal-400">ONESHIM</span>
              <div className="flex items-center space-x-2">
                {primaryNavItems.map((item) => (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    className={({ isActive }) =>
                      `px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                        isActive
                          ? 'bg-slate-200 dark:bg-slate-700 text-slate-900 dark:text-white'
                          : 'text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700 hover:text-slate-900 dark:hover:text-white'
                      }`
                    }
                  >
                    <span className="hidden sm:inline">{t(item.labelKey)}</span>
                    <span className="sm:hidden">{item.mobileLabel}</span>
                  </NavLink>
                ))}

                <div className="relative" ref={moreMenuRef}>
                  <button
                    type="button"
                    className={`px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                      isSecondaryActive || showMoreMenu
                        ? 'bg-slate-200 dark:bg-slate-700 text-slate-900 dark:text-white'
                        : 'text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700 hover:text-slate-900 dark:hover:text-white'
                    }`}
                    onClick={() => setShowMoreMenu((prev) => !prev)}
                    aria-haspopup="menu"
                    aria-expanded={showMoreMenu}
                  >
                    {t('common.more', '더보기')}
                  </button>

                  {showMoreMenu && (
                    <div
                      role="menu"
                      className="absolute top-11 left-0 min-w-44 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shadow-lg p-1 z-40"
                    >
                      {secondaryNavItems.map((item) => (
                        <NavLink
                          key={item.to}
                          to={item.to}
                          role="menuitem"
                          onClick={() => setShowMoreMenu(false)}
                          className={({ isActive }) =>
                            `block px-3 py-2 rounded-md text-sm transition-colors ${
                              isActive
                                ? 'bg-slate-200 dark:bg-slate-700 text-slate-900 dark:text-white'
                                : 'text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 hover:text-slate-900 dark:hover:text-white'
                            }`
                          }
                        >
                          {t(item.labelKey)}
                        </NavLink>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
            {/* 글로벌 검색 */}
            <form onSubmit={handleGlobalSearch} className="hidden md:flex items-center">
              <input
                type="text"
                value={searchInput}
                onChange={(e) => setSearchInput(e.target.value)}
                placeholder={`${t('common.search')}... (Enter)`}
                className="w-48 bg-slate-200 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg px-3 py-1.5 text-sm text-slate-900 dark:text-white placeholder-slate-500 dark:placeholder-slate-400 focus:outline-none focus:border-teal-500 dark:focus:border-teal-500 transition-colors"
              />
            </form>
            <div className="flex items-center space-x-1 sm:space-x-2">
              {/* 언어 선택기 */}
              <LanguageSelector />
              {/* 단축키 도움말 버튼 */}
              <button
                onClick={() => setShowShortcutsHelp(true)}
                className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors"
                title={`${t('shortcuts.title')} (?)`}
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </button>
              {/* 테마 토글 버튼 */}
              <button
                onClick={toggleTheme}
                className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors"
                title={theme === 'dark' ? 'Light mode' : 'Dark mode'}
              >
                {theme === 'dark' ? (
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
                  </svg>
                ) : (
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
                  </svg>
                )}
              </button>
              {/* 상태 표시 */}
              <div className="hidden sm:flex items-center space-x-2 text-sm text-slate-600 dark:text-slate-400">
                <span className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></span>
                <span>{t('common.running')}</span>
              </div>
            </div>
          </div>
        </div>
      </nav>

      {/* 메인 콘텐츠 */}
      <main className="max-w-7xl mx-auto px-4 py-6">
        <ErrorBoundary>
          <Suspense
            fallback={
              <div className="flex items-center justify-center min-h-[60vh]">
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

      {/* 단축키 도움말 모달 */}
      {showShortcutsHelp && (
        <ShortcutsHelp onClose={() => setShowShortcutsHelp(false)} />
      )}
    </div>
  )
}

export default App
