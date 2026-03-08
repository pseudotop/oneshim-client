// Language selector component
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { changeLanguage, getCurrentLanguage, type SupportedLanguageCode, supportedLanguages } from '../i18n'
import { elevation } from '../styles/tokens'
import { cn } from '../utils/cn'

export default function LanguageSelector() {
  const { t } = useTranslation()
  const [isOpen, setIsOpen] = useState(false)
  const [currentLang, setCurrentLang] = useState(getCurrentLanguage())
  const dropdownRef = useRef<HTMLDivElement>(null)

  // Close dropdown on outside click
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const handleLanguageChange = (lng: SupportedLanguageCode) => {
    changeLanguage(lng)
    setCurrentLang(lng)
    setIsOpen(false)
  }

  const currentLanguage = supportedLanguages.find((l) => l.code === currentLang)

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-content-secondary text-sm transition-colors hover:bg-hover"
        title={t('settings.language')}
      >
        <span className="hidden sm:inline">{currentLanguage?.name}</span>
        <span className="sm:hidden">{currentLanguage?.code.split('-')[0].toUpperCase()}</span>
        <svg
          className={`h-4 w-4 transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isOpen && (
        <div
          className={cn(
            'absolute right-0 mt-1 w-36 rounded-lg border border-muted bg-surface-overlay py-1',
            elevation.dropdown,
          )}
        >
          {supportedLanguages.map((lang) => (
            <button
              type="button"
              key={lang.code}
              onClick={() => handleLanguageChange(lang.code as SupportedLanguageCode)}
              className={`flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors ${
                currentLang === lang.code ? 'bg-teal-500/10 text-accent-teal' : 'text-content-strong hover:bg-hover'
              }`}
            >
              <span>{lang.name}</span>
              {currentLang === lang.code && (
                <svg className="ml-auto h-4 w-4" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                  <path
                    fillRule="evenodd"
                    d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                    clipRule="evenodd"
                  />
                </svg>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
