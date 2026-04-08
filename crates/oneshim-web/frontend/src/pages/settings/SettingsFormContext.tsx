import { createContext, useContext, useEffect, useState } from 'react'
import type { AppSettings } from '../../api/client'
import type { SettingsDataResult } from '../hooks/useSettingsData'
import { useSettingsData } from '../hooks/useSettingsData'
import type { SettingsFormResult } from '../hooks/useSettingsForm'
import { useSettingsForm } from '../hooks/useSettingsForm'

export interface SettingsContextValue {
  form: SettingsFormResult
  data: SettingsDataResult
}

const SettingsFormContext = createContext<SettingsContextValue | null>(null)

export function SettingsFormProvider({ children }: { children: React.ReactNode }) {
  const [formDataForProbes, setFormDataForProbes] = useState<AppSettings | null>(null)
  const data = useSettingsData(formDataForProbes)
  const form = useSettingsForm(data)

  useEffect(() => {
    setFormDataForProbes(form.formData)
  }, [form.formData])

  // Guard against silent data loss when the page is reloaded (browser refresh
  // or Rust-triggered full-reload recovery) while the user has unsaved
  // settings changes. The browser shows a generic confirm dialog.
  useEffect(() => {
    if (!form.hasUnsavedChanges) return
    const handler = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      // Modern browsers ignore the returnValue but require it set for the
      // dialog to appear at all.
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [form.hasUnsavedChanges])

  // PERF: the context value is a fresh object literal on every render because
  // `form` (from useSettingsForm) and `data` (from useSettingsData) return
  // fresh objects on every render — most of their handlers are not yet
  // wrapped in useCallback, and their return object isn't useMemo'd. A
  // downstream `useMemo(() => ({ form, data }), [form, data])` would be a
  // no-op (the deps would change every render). Fixing this requires a
  // larger refactor: wrap ~25 handlers in useSettingsForm and ~8 in
  // useSettingsData in useCallback, then useMemo both hook return objects,
  // then useMemo the context value. Tracked as follow-up — currently all
  // 9 tab consumers re-render on every keystroke, same as before.
  return <SettingsFormContext.Provider value={{ form, data }}>{children}</SettingsFormContext.Provider>
}

export function useSettingsFormContext(): SettingsContextValue {
  const ctx = useContext(SettingsFormContext)
  if (!ctx) throw new Error('useSettingsFormContext must be used inside SettingsFormProvider')
  return ctx
}
