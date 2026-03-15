import type {
  FeatureAvailability,
  FeatureCapabilitySnapshot,
  ProviderSurfaceCatalog,
  ProviderSurfaceSpec,
} from '../api/contracts'

export type EndpointSurfaceKind = 'ocr_api' | 'llm_api'

const STABILITY_RANK: Record<string, number> = {
  ga: 3,
  preview: 2,
  experimental: 1,
  deprecated: 0,
}

const AVAILABILITY_RANK: Record<FeatureAvailability, number> = {
  available: 2,
  partially_available: 1,
  unavailable: 0,
}

function normalizedProviderType(providerType: string | null | undefined): string {
  return (providerType ?? '').trim() || 'Generic'
}

function expectedExecutionKind(accessMode: string | null | undefined, endpointKind: EndpointSurfaceKind): string | null {
  if (accessMode === 'ProviderSubscriptionCli') {
    return endpointKind === 'llm_api' ? 'subprocess_cli' : null
  }

  if (accessMode === 'ProviderOAuth' && endpointKind === 'llm_api') {
    return 'managed_http'
  }

  return 'direct_http'
}

function surfaceSupportsKind(surface: ProviderSurfaceSpec, endpointKind: EndpointSurfaceKind): boolean {
  return endpointKind === 'ocr_api' ? surface.supports.ocr : surface.supports.llm
}

function featureAvailabilityScore(
  surface: ProviderSurfaceSpec,
  snapshot: FeatureCapabilitySnapshot | null | undefined,
): number {
  if (surface.execution_kind === 'direct_http') {
    return AVAILABILITY_RANK.available
  }

  const feature = snapshot?.features.find((candidate) => candidate.feature_id === surface.surface_id)
  if (!feature) {
    return AVAILABILITY_RANK.partially_available
  }

  return AVAILABILITY_RANK[feature.availability]
}

function compareProviderSurfaces(
  left: ProviderSurfaceSpec,
  right: ProviderSurfaceSpec,
  snapshot?: FeatureCapabilitySnapshot | null,
): number {
  const availabilityDelta = featureAvailabilityScore(right, snapshot) - featureAvailabilityScore(left, snapshot)
  if (availabilityDelta !== 0) {
    return availabilityDelta
  }

  if (left.preferred_for_product_auth !== right.preferred_for_product_auth) {
    return Number(right.preferred_for_product_auth) - Number(left.preferred_for_product_auth)
  }

  const stabilityDelta = (STABILITY_RANK[right.stability] ?? -1) - (STABILITY_RANK[left.stability] ?? -1)
  if (stabilityDelta !== 0) {
    return stabilityDelta
  }

  return left.display_name.localeCompare(right.display_name)
}

export function sortProviderSurfaces(
  surfaces: ProviderSurfaceSpec[],
  snapshot?: FeatureCapabilitySnapshot | null,
): ProviderSurfaceSpec[] {
  return [...surfaces].sort((left, right) => compareProviderSurfaces(left, right, snapshot))
}

export function getCompatibleProviderSurfaces(
  catalog: ProviderSurfaceCatalog,
  accessMode: string | null | undefined,
  endpointKind: EndpointSurfaceKind,
): ProviderSurfaceSpec[] {
  const executionKind = expectedExecutionKind(accessMode, endpointKind)
  if (!executionKind) {
    return []
  }

  return sortProviderSurfaces(
    catalog.surfaces.filter(
      (surface) => surface.execution_kind === executionKind && surfaceSupportsKind(surface, endpointKind),
    ),
  )
}

export function deriveDefaultProviderSurfaceId(
  catalog: ProviderSurfaceCatalog,
  accessMode: string | null | undefined,
  endpointKind: EndpointSurfaceKind,
  providerType: string | null | undefined,
): string | null {
  const normalizedProvider = normalizedProviderType(providerType)
  const compatible = getCompatibleProviderSurfaces(catalog, accessMode, endpointKind)
  const vendorMatch = compatible.filter((surface) => surface.provider_type === normalizedProvider)
  const candidates = sortProviderSurfaces(vendorMatch.length > 0 ? vendorMatch : compatible)

  return candidates[0]?.surface_id ?? null
}

export function providerSurfaceById(
  catalog: ProviderSurfaceCatalog,
  surfaceId: string | null | undefined,
): ProviderSurfaceSpec | undefined {
  const normalized = (surfaceId ?? '').trim().toLowerCase()
  if (!normalized) {
    return undefined
  }

  return catalog.surfaces.find((surface) => surface.surface_id.toLowerCase() === normalized)
}

export function resolveProviderTypeForSurface(
  catalog: ProviderSurfaceCatalog,
  surfaceId: string | null | undefined,
  fallbackProviderType?: string | null,
): string {
  return providerSurfaceById(catalog, surfaceId)?.provider_type ?? normalizedProviderType(fallbackProviderType)
}

export function defaultSurfaceEndpoint(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
): string {
  if (!surface) {
    return ''
  }

  const transport = endpointKind === 'ocr_api' ? surface.ocr_transport : surface.llm_transport
  return transport?.url ?? ''
}

export function defaultSurfaceModel(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
): string | null {
  if (!surface) {
    return null
  }

  const models = endpointKind === 'ocr_api' ? surface.default_models.ocr_models : surface.default_models.llm_models
  return models[0] ?? null
}
