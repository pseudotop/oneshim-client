import { screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../__tests__/helpers/render-helpers'
import AdvancedTab from './AdvancedTab'
import AudioTab from './AudioTab'
import DataStorageTab from './DataStorageTab'
import MonitoringTab from './MonitoringTab'
import NotificationSettings from './NotificationSettings'
import PrivacySettings from './PrivacySettings'
import SyncTab from './SyncTab'
import { makeDefaultFormData } from './stories-utils'

const mockUseSettingsFormContext = vi.hoisted(() => vi.fn())
const mockUseLoadedFormData = vi.hoisted(() => vi.fn())
const mockInvoke = vi.hoisted(() => vi.fn())

vi.mock('../settings/SettingsFormContext', () => ({
  useSettingsFormContext: mockUseSettingsFormContext,
  useLoadedFormData: mockUseLoadedFormData,
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

function mockSettingsContext() {
  const formData = makeDefaultFormData()
  mockUseLoadedFormData.mockReturnValue(formData)
  mockUseSettingsFormContext.mockReturnValue({
    form: {
      formData,
      exportFormat: 'json',
      exportLoading: null,
      handleExport: vi.fn(),
      handleNotificationChange: vi.fn(),
      handlePrivacyChange: vi.fn(),
      handleRootChange: vi.fn(),
      handleMonitorChange: vi.fn(),
      handleTelemetryChange: vi.fn(),
      requestNotificationPermissionMutation: { isPending: false, mutate: vi.fn() },
      setExportFormat: vi.fn(),
      setFormData: vi.fn(),
    },
    data: {
      canQueryDesktopCapabilities: false,
      desktopPermissionStatus: null,
      desktopPermissionStatusError: null,
      desktopPermissionStatusLoading: false,
      desktopPermissionStatusRefreshing: false,
      handleRefreshDesktopPermissionStatus: vi.fn(),
      storageLoading: false,
      storageStats: null,
    },
  })
  return formData
}

describe('Settings guidance copy', () => {
  beforeEach(() => {
    mockUseSettingsFormContext.mockReset()
    mockUseLoadedFormData.mockReset()
    mockInvoke.mockReset()
  })

  it('orients the data storage page around review, export, and retention decisions', () => {
    mockSettingsContext()

    renderWithProviders(<DataStorageTab />)

    expect(screen.getByRole('region', { name: 'Data & storage guide' })).toBeInTheDocument()
    expect(screen.getByText('Export before reducing retention')).toBeInTheDocument()
    expect(screen.getByText('Telemetry is separate')).toBeInTheDocument()
  })

  it('orients monitoring controls around permissions, intervals, and privacy mode', () => {
    mockSettingsContext()

    renderWithProviders(<MonitoringTab />)

    expect(screen.getByRole('region', { name: 'Monitoring guide' })).toBeInTheDocument()
    expect(screen.getByText('Resolve desktop access first')).toBeInTheDocument()
    expect(screen.getByText('Use privacy mode for pauses')).toBeInTheDocument()
  })

  it('orients privacy controls before users edit app and title exclusions', () => {
    const formData = makeDefaultFormData()

    renderWithProviders(<PrivacySettings privacy={formData.privacy} onChange={vi.fn()} />)

    expect(screen.getByRole('region', { name: 'Privacy guide' })).toBeInTheDocument()
    expect(screen.getByText('Start with automatic exclusions')).toBeInTheDocument()
    expect(screen.getByText('Use title patterns for sensitive workflows')).toBeInTheDocument()
  })

  it('orients notification thresholds around permission state and interruption cost', () => {
    const formData = makeDefaultFormData()

    renderWithProviders(<NotificationSettings notification={formData.notification} onChange={vi.fn()} />)

    expect(screen.getByRole('region', { name: 'Notification guide' })).toBeInTheDocument()
    expect(screen.getByText('Confirm OS permission first')).toBeInTheDocument()
    expect(screen.getByText('Keep high-usage alerts rare')).toBeInTheDocument()
  })

  it('orients audio setup around provider choice, model footprint, and input mode', () => {
    mockSettingsContext()

    renderWithProviders(<AudioTab />)

    expect(screen.getByRole('region', { name: 'Audio setup guide' })).toBeInTheDocument()
    expect(screen.getByText('Choose local or cloud STT')).toBeInTheDocument()
    expect(screen.getByText('Pick an input mode')).toBeInTheDocument()
  })

  it('orients advanced settings around runtime, network, and sync impact', () => {
    mockSettingsContext()

    renderWithProviders(<AdvancedTab />)

    expect(screen.getByRole('region', { name: 'Advanced settings guide' })).toBeInTheDocument()
    expect(screen.getByText('Change runtime limits carefully')).toBeInTheDocument()
    expect(screen.getByText('Pair sync settings with the sync page')).toBeInTheDocument()
  })

  it('orients sync setup when sync is disabled', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_sync_status') {
        return Promise.resolve({
          enabled: false,
          device_id: 'device-1',
          device_name: 'Work Mac',
        })
      }
      return Promise.resolve([])
    })

    renderWithProviders(<SyncTab />)

    await waitFor(() => {
      expect(screen.getByRole('region', { name: 'Sync setup guide' })).toBeInTheDocument()
    })
    expect(screen.getByText('Choose a transport deliberately')).toBeInTheDocument()
    expect(screen.getByText('Protect the passphrase')).toBeInTheDocument()
  })
})
