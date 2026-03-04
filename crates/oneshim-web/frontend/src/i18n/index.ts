// i18n setup
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'

import ko from './locales/ko.json'
import en from './locales/en.json'

const resources = {
  ko: { translation: ko },
  en: { translation: en },
}

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: 'en',
    supportedLngs: ['ko', 'en'],

    detection: {
      order: ['localStorage'],
      lookupLocalStorage: 'oneshim-language',
      caches: ['localStorage'],
    },

    interpolation: {
      escapeValue: false, // React handles escaping
    },

    react: {
      useSuspense: false, // Used without SSR
    },
  })

export default i18n

export type SupportedLanguageCode = 'ko' | 'en'

// Language change helper
export const changeLanguage = (lng: SupportedLanguageCode) => {
  i18n.changeLanguage(lng)
  localStorage.setItem('oneshim-language', lng)
}

// Get current language
export const getCurrentLanguage = (): SupportedLanguageCode => {
  const lng = i18n.language
  return (['ko', 'en'] as const).includes(lng as SupportedLanguageCode)
    ? (lng as SupportedLanguageCode)
    : 'en'
}

// Supported language list
export const supportedLanguages = [
  { code: 'en', name: 'English' },
  { code: 'ko', name: '한국어' },
] as const
