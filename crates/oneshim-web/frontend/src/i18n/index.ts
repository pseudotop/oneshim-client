// i18n 설정
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
      escapeValue: false, // React에서 XSS 방지됨
    },

    react: {
      useSuspense: false, // SSR 없이 사용
    },
  })

export default i18n

// 언어 변경 헬퍼
export const changeLanguage = (lng: 'ko' | 'en') => {
  i18n.changeLanguage(lng)
  localStorage.setItem('oneshim-language', lng)
}

// 현재 언어 가져오기
export const getCurrentLanguage = (): 'ko' | 'en' => {
  const lng = i18n.language
  return lng === 'ko' ? 'ko' : 'en'
}

// 지원 언어 목록
export const supportedLanguages = [
  { code: 'en', name: 'English', flag: '🇺🇸' },
  { code: 'ko', name: '한국어', flag: '🇰🇷' },
] as const
