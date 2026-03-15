import type { FeatureCapability, FeatureCapabilitySnapshot } from '../api/contracts'

export function findFeatureCapability(
  snapshot: FeatureCapabilitySnapshot | null | undefined,
  featureId: string,
): FeatureCapability | null {
  return snapshot?.features.find((feature) => feature.feature_id === featureId) ?? null
}

export function maturityBadgeColor(
  maturity: FeatureCapability['maturity'],
): 'success' | 'warning' | 'error' | 'default' {
  switch (maturity) {
    case 'stable':
      return 'success'
    case 'beta':
      return 'warning'
    case 'experimental':
      return 'error'
    case 'deprecated':
      return 'default'
  }
}
