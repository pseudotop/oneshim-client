// i18n setup
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'

import ko from './locales/ko.json'
import en from './locales/en.json'
import ja from './locales/ja.json'
import zh from './locales/zh.json'

const resources = {
  ko: { translation: ko },
  en: { translation: en },
  ja: { translation: ja },
  zh: { translation: zh },
}

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: 'en',
    supportedLngs: ['ko', 'en', 'ja', 'zh'],

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

export type SupportedLanguageCode = 'ko' | 'en' | 'ja' | 'zh'

// Language change helper
export const changeLanguage = (lng: SupportedLanguageCode) => {
  i18n.changeLanguage(lng)
  localStorage.setItem('oneshim-language', lng)
}

// Get current language
export const getCurrentLanguage = (): SupportedLanguageCode => {
  const lng = i18n.language
  return (['ko', 'en', 'ja', 'zh'] as const).includes(lng as SupportedLanguageCode)
    ? (lng as SupportedLanguageCode)
    : 'en'
}

// Supported language list
export const supportedLanguages = [
  { code: 'en', name: 'English' },
  { code: 'ko', name: '한국어' },
  { code: 'ja', name: '日本語' },
  { code: 'zh', name: '中文' },
] as const
