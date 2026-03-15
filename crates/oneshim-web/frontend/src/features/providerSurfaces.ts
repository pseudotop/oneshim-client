import type { ProviderSurfaceCatalog, ProviderSurfaceSpec } from '../api/contracts'

export type EndpointSurfaceKind = 'ocr_api' | 'llm_api'

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

export function getCompatibleProviderSurfaces(
  catalog: ProviderSurfaceCatalog,
  accessMode: string | null | undefined,
  endpointKind: EndpointSurfaceKind,
): ProviderSurfaceSpec[] {
  const executionKind = expectedExecutionKind(accessMode, endpointKind)
  if (!executionKind) {
    return []
  }

  return catalog.surfaces.filter(
    (surface) => surface.execution_kind === executionKind && surfaceSupportsKind(surface, endpointKind),
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
  const candidates = vendorMatch.length > 0 ? vendorMatch : compatible

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
