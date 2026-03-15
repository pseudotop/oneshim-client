import { describe, expect, it } from 'vitest'
import { findFeatureCapability, maturityBadgeColor } from '../featureCapabilities'

describe('featureCapabilities helpers', () => {
  it('finds a feature by id', () => {
    const feature = findFeatureCapability(
      {
        features: [
          {
            feature_id: 'provider_surface.openai.managed_oauth',
            maturity: 'experimental',
            availability: 'available',
            preferred: false,
            requires: ['os_secret_store'],
            status_reason: null,
            status_copy_key: null,
          },
        ],
      },
      'provider_surface.openai.managed_oauth',
    )

    expect(feature?.maturity).toBe('experimental')
  })

  it('maps maturity to badge color', () => {
    expect(maturityBadgeColor('stable')).toBe('success')
    expect(maturityBadgeColor('beta')).toBe('warning')
    expect(maturityBadgeColor('experimental')).toBe('error')
    expect(maturityBadgeColor('deprecated')).toBe('default')
  })
})
