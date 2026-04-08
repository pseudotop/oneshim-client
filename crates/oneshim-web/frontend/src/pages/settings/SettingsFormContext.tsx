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

  return <SettingsFormContext.Provider value={{ form, data }}>{children}</SettingsFormContext.Provider>
}

export function useSettingsFormContext(): SettingsContextValue {
  const ctx = useContext(SettingsFormContext)
  if (!ctx) throw new Error('useSettingsFormContext must be used inside SettingsFormProvider')
  return ctx
}
