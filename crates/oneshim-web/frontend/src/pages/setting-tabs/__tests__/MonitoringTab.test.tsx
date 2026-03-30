import { screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import MonitoringTab from '../MonitoringTab'
import { makeDefaultFormData } from '../stories-utils'

describe('MonitoringTab', () => {
  it('renders macOS permission states with actionable badges', () => {
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
            state: 'granted',
            status_reason: 'macos_notifications_granted',
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
    expect(screen.getAllByText('Ready').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Action recommended')).toBeInTheDocument()
  })

  it('renders a request action when macOS notification permission has not been requested yet', () => {
    const requestPermission = vi.fn()

    renderWithProviders(
      <MonitoringTab
        formData={makeDefaultFormData()}
        permissionStatus={{
          platform: 'macos',
          accessibility: {
            state: 'granted',
            status_reason: 'macos_accessibility_granted',
          },
          screen_capture: {
            state: 'granted',
            status_reason: 'macos_screen_capture_granted',
          },
          notifications: {
            state: 'needs_attention',
            status_reason: 'macos_notifications_not_determined',
          },
        }}
        permissionStatusRefreshing={false}
        onRootChange={() => {}}
        onMonitorChange={() => {}}
        onRefreshPermissionStatus={() => {}}
        onRequestNotificationPermission={requestPermission}
      />,
    )

    expect(screen.getByRole('button', { name: 'Request permission' })).toBeInTheDocument()
    expect(screen.getByText('Attention needed')).toBeInTheDocument()
  })

  it('renders the Windows guidance copy instead of macOS permission rows', () => {
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
