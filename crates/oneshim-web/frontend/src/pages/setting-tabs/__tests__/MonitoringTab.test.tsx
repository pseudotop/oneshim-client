import { screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import MonitoringTab from '../MonitoringTab'
import { makeDefaultFormData } from '../stories-utils'

const originalNotification = window.Notification

describe('MonitoringTab', () => {
  afterEach(() => {
    Object.defineProperty(window, 'Notification', {
      configurable: true,
      value: originalNotification,
    })
  })

  it('renders macOS permission states with actionable badges', () => {
    Object.defineProperty(window, 'Notification', {
      configurable: true,
      value: { permission: 'granted' },
    })

    renderWithProviders(
      <MonitoringTab
        formData={makeDefaultFormData()}
        permissionStatus={{
          platform: 'macos',
          accessibility: {
            state: 'needs_attention',
            status_reason: 'macos_accessibility_missing',
          },
          screen_capture: {
            state: 'granted',
            status_reason: 'macos_screen_capture_granted',
          },
          notifications: {
            state: 'unavailable',
            status_reason: 'macos_notifications_status_unverifiable',
          },
        }}
        permissionStatusRefreshing={false}
        onRootChange={() => {}}
        onMonitorChange={() => {}}
        onRefreshPermissionStatus={() => {}}
      />,
    )

    expect(screen.getByText('macOS Permissions')).toBeInTheDocument()
    expect(screen.getByText('Attention needed')).toBeInTheDocument()
    expect(screen.getByText('Unavailable')).toBeInTheDocument()
    expect(screen.getAllByText('Ready').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Action recommended')).toBeInTheDocument()
  })

  it('renders the Windows guidance copy instead of macOS permission rows', () => {
    Object.defineProperty(window, 'Notification', {
      configurable: true,
      value: { permission: 'default' },
    })

    renderWithProviders(
      <MonitoringTab
        formData={makeDefaultFormData()}
        permissionStatus={{
          platform: 'windows',
          accessibility: {
            state: 'not_required',
            status_reason: 'windows_uia_no_permission_required',
          },
          screen_capture: {
            state: 'not_required',
            status_reason: 'screen_capture_ready',
          },
          notifications: {
            state: 'not_required',
            status_reason: 'windows_notifications_managed_by_os',
          },
        }}
        permissionStatusRefreshing={false}
        onRootChange={() => {}}
        onMonitorChange={() => {}}
        onRefreshPermissionStatus={() => {}}
      />,
    )

    expect(screen.getByText('Windows access')).toBeInTheDocument()
    expect(screen.queryByText('macOS Permissions')).not.toBeInTheDocument()
  })

  it('renders a permission probe failure with a retry action', () => {
    renderWithProviders(
      <MonitoringTab
        formData={makeDefaultFormData()}
        permissionStatusError="desktop permission probe failed"
        permissionStatusRefreshing={false}
        onRootChange={() => {}}
        onMonitorChange={() => {}}
        onRefreshPermissionStatus={() => {}}
      />,
    )

    expect(screen.getByText('Desktop access check failed')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Refresh status' })).toBeInTheDocument()
  })
})
