/**
 * Settings page — tabbed interface with 5 tabs.
 */
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Settings2 } from 'lucide-react'
import { Tabs } from '../components/ui'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import {
  AiAutomationTab,
  DataStorageTab,
  GeneralTab,
  MonitoringTab,
  PrivacyTab,
} from './settingSections'

export default function Settings() {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState('general')

  const tabs = [
    { id: 'general', label: t('settings.tabs.general') },
    { id: 'privacy', label: t('settings.tabs.privacy') },
    { id: 'monitoring', label: t('settings.tabs.monitoring') },
    { id: 'ai-automation', label: t('settings.tabs.aiAutomation') },
    { id: 'data', label: t('settings.tabs.dataStorage') },
  ]

  return (
    <div className="min-h-full space-y-6 p-6">
      <h1 className={cn(typography.h1, colors.text.primary)}>
        <Settings2 className="mr-2 inline h-6 w-6" />
        {t('settings.title')}
      </h1>

      <Tabs tabs={tabs} activeTab={activeTab} onTabChange={setActiveTab} />

      <div className="space-y-6">
        {activeTab === 'general' && <GeneralTab />}
        {activeTab === 'privacy' && <PrivacyTab />}
        {activeTab === 'monitoring' && <MonitoringTab />}
        {activeTab === 'ai-automation' && <AiAutomationTab />}
        {activeTab === 'data' && <DataStorageTab />}
      </div>
    </div>
  )
}
