import type {
  FeatureAvailability,
  FeatureCapabilitySnapshot,
  ProviderKnownModelSpec,
  ProviderSurfaceCatalog,
  ProviderSurfaceSpec,
} from '../api/contracts'

export type EndpointSurfaceKind = 'ocr_api' | 'llm_api'
export type UnknownModelPolicy = 'allow' | 'warn' | 'reject'

const STABILITY_RANK: Record<string, number> = {
  ga: 3,
  preview: 2,
  experimental: 1,
  deprecated: 0,
}

const PLACEMENT_RANK: Record<string, number> = {
  self_hosted: 3,
  installed_cli: 2,
  provider_hosted: 1,
  custom_hosted: 0,
}

const AVAILABILITY_RANK: Record<FeatureAvailability, number> = {
  available: 2,
  partially_available: 1,
  unavailable: 0,
}

function normalizedProviderType(providerType: string | null | undefined): string {
  return (providerType ?? '').trim() || 'Generic'
}

function expectedExecutionKind(accessMode: string | null | undefined, _endpointKind: EndpointSurfaceKind): string | null {
  if (_endpointKind === 'ocr_api') {
    return null
  }

  if (accessMode === 'ProviderSubscriptionCli') {
    return 'subprocess_cli'
  }

  if (accessMode === 'ProviderOAuth') {
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
  const feature = snapshot?.features.find((candidate) => candidate.feature_id === surface.surface_id)
  if (feature) {
    return AVAILABILITY_RANK[feature.availability]
  }

  if (surface.execution_kind === 'direct_http') {
    return AVAILABILITY_RANK.available
  }

  return AVAILABILITY_RANK.partially_available
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

  const placementDelta = (PLACEMENT_RANK[right.placement_kind] ?? -1) - (PLACEMENT_RANK[left.placement_kind] ?? -1)
  if (placementDelta !== 0) {
    return placementDelta
  }

  return left.display_name.localeCompare(right.display_name)
}

function compareOcrDefaultSurfaces(
  left: ProviderSurfaceSpec,
  right: ProviderSurfaceSpec,
  snapshot?: FeatureCapabilitySnapshot | null,
): number {
  const directHttpDelta = Number(right.execution_kind === 'direct_http') - Number(left.execution_kind === 'direct_http')
  if (directHttpDelta !== 0) {
    return directHttpDelta
  }

  return compareProviderSurfaces(left, right, snapshot)
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
  snapshot?: FeatureCapabilitySnapshot | null,
): ProviderSurfaceSpec[] {
  const executionKind = expectedExecutionKind(accessMode, endpointKind)

  return sortProviderSurfaces(
    catalog.surfaces.filter(
      (surface) =>
        (executionKind == null || surface.execution_kind === executionKind) &&
        surfaceSupportsKind(surface, endpointKind),
    ),
    snapshot,
  )
}

export function deriveDefaultProviderSurfaceId(
  catalog: ProviderSurfaceCatalog,
  accessMode: string | null | undefined,
  endpointKind: EndpointSurfaceKind,
  providerType: string | null | undefined,
  snapshot?: FeatureCapabilitySnapshot | null,
): string | null {
  const normalizedProvider = normalizedProviderType(providerType)
  const compatible = getCompatibleProviderSurfaces(catalog, accessMode, endpointKind, snapshot)
  const vendorMatch = compatible.filter((surface) => surface.provider_type === normalizedProvider)
  const rawCandidates = vendorMatch.length > 0 ? vendorMatch : compatible
  const candidates =
    endpointKind === 'ocr_api'
      ? [...rawCandidates].sort((left, right) => compareOcrDefaultSurfaces(left, right, snapshot))
      : sortProviderSurfaces(rawCandidates, snapshot)

  return candidates[0]?.surface_id ?? null
}

export function providerSurfaceById(
  catalog: ProviderSurfaceCatalog,
  surfaceId: string | null | undefined,
): ProviderSurfaceSpec | undefined {
  return providerSurfaceByIdFromList(catalog.surfaces, surfaceId)
}

export function providerSurfaceByIdFromList(
  surfaces: ProviderSurfaceSpec[],
  surfaceId: string | null | undefined,
): ProviderSurfaceSpec | undefined {
  const normalized = (surfaceId ?? '').trim().toLowerCase()
  if (!normalized) {
    return undefined
  }

  return surfaces.find((surface) => surface.surface_id.toLowerCase() === normalized)
}

export function relatedProviderSurfaces(
  catalog: ProviderSurfaceCatalog,
  surface: ProviderSurfaceSpec | null | undefined,
): ProviderSurfaceSpec[] {
  return relatedProviderSurfacesFromList(catalog.surfaces, surface)
}

export function relatedProviderSurfacesFromList(
  surfaces: ProviderSurfaceSpec[],
  surface: ProviderSurfaceSpec | null | undefined,
): ProviderSurfaceSpec[] {
  if (!surface) {
    return []
  }

  return surface.related_surface_ids
    .map((surfaceId) => providerSurfaceByIdFromList(surfaces, surfaceId))
    .filter((candidate): candidate is ProviderSurfaceSpec => candidate != null)
}

export function preferredRelatedProviderSurface(
  catalog: ProviderSurfaceCatalog,
  surface: ProviderSurfaceSpec | null | undefined,
  executionKind?: string,
  snapshot?: FeatureCapabilitySnapshot | null,
): ProviderSurfaceSpec | undefined {
  return preferredRelatedProviderSurfaceFromList(catalog.surfaces, surface, executionKind, snapshot)
}

export function preferredRelatedProviderSurfaceFromList(
  surfaces: ProviderSurfaceSpec[],
  surface: ProviderSurfaceSpec | null | undefined,
  executionKind?: string,
  snapshot?: FeatureCapabilitySnapshot | null,
): ProviderSurfaceSpec | undefined {
  const related = relatedProviderSurfacesFromList(surfaces, surface).filter(
    (candidate) => executionKind == null || candidate.execution_kind === executionKind,
  )

  return sortProviderSurfaces(related, snapshot)[0]
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

export function surfaceSupportsParameter(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
  parameter: string,
): boolean {
  if (!surface) {
    return false
  }

  const normalized = parameter.trim().toLowerCase()
  if (!normalized) {
    return false
  }

  const profile = endpointKind === 'ocr_api' ? surface.parameter_profiles.ocr : surface.parameter_profiles.llm
  return profile.supported.some((candidate) => candidate.toLowerCase() === normalized)
}

export function surfaceSupportsModelSelection(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
): boolean {
  if (!surface) {
    return false
  }

  const defaults =
    endpointKind === 'ocr_api' ? surface.default_models.ocr_models : surface.default_models.llm_models
  const modelCatalogSupported = endpointKind === 'ocr_api'
    ? (surface.model_catalog_transport?.ocr_supported ?? false)
    : (surface.model_catalog_transport?.llm_supported ?? false)
  const knownModels = surface.known_models.some((model) =>
    endpointKind === 'ocr_api' ? model.capabilities.ocr : model.capabilities.llm,
  )

  return defaults.length > 0 || modelCatalogSupported || knownModels
}

function matchesKnownModel(model: ProviderKnownModelSpec, value: string): boolean {
  const normalized = value.trim().toLowerCase()
  if (!normalized) {
    return false
  }

  if (model.id.toLowerCase() === normalized) {
    return true
  }

  if (model.aliases.some((alias) => alias.toLowerCase() === normalized)) {
    return true
  }

  return model.id_prefixes.some((prefix) => {
    const normalizedPrefix = prefix.trim().toLowerCase()
    return normalizedPrefix.length > 0 && (normalized === normalizedPrefix || normalized.startsWith(normalizedPrefix))
  })
}

export function surfaceKnownModel(
  surface: ProviderSurfaceSpec | undefined,
  model: string | null | undefined,
): ProviderKnownModelSpec | undefined {
  const candidate = (model ?? '').trim()
  if (!surface || !candidate) {
    return undefined
  }

  return surface.known_models.find((entry) => matchesKnownModel(entry, candidate))
}

export function surfaceModelSupportsCapability(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
  model: string | null | undefined,
): boolean | null {
  const known = surfaceKnownModel(surface, model)
  if (!known) {
    return null
  }

  return endpointKind === 'ocr_api' ? known.capabilities.ocr : known.capabilities.llm
}

export function surfaceUnknownModelPolicy(
  surface: ProviderSurfaceSpec | undefined,
  endpointKind: EndpointSurfaceKind,
): UnknownModelPolicy {
  if (!surface) {
    return 'warn'
  }

  return endpointKind === 'ocr_api'
    ? surface.unknown_model_policy?.ocr ?? 'warn'
    : surface.unknown_model_policy?.llm ?? 'warn'
}
