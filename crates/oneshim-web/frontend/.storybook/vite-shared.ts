import type { ManualChunkMeta, OutputOptions } from 'rollup'
import { mergeConfig, type InlineConfig } from 'vite'

function packageNameFromModuleId(id: string): string | null {
  const nodeModulesMarker = '/node_modules/'
  const nodeModulesIndex = id.lastIndexOf(nodeModulesMarker)
  if (nodeModulesIndex === -1) {
    return null
  }

  const modulePath = id.slice(nodeModulesIndex + nodeModulesMarker.length)
  const segments = modulePath.split('/')
  if (segments.length === 0) {
    return null
  }

  if (segments[0].startsWith('@') && segments.length >= 2) {
    return `${segments[0]}/${segments[1]}`
  }

  return segments[0] || null
}

function sanitizeChunkName(packageName: string): string {
  return packageName.replace(/^@/, '').replace(/[\\/]/g, '-')
}

function resolveStorybookChunk(id: string): string | undefined {
  const packageName = packageNameFromModuleId(id)
  if (!packageName) {
    return undefined
  }

  if (packageName === 'react' || packageName === 'react-dom' || packageName === 'scheduler') {
    return 'sb-react-vendor'
  }

  if (packageName === 'react-router' || packageName === 'react-router-dom') {
    return 'sb-app-router'
  }

  if (packageName === '@tanstack/react-query') {
    return 'sb-app-query'
  }

  if (packageName === 'recharts' || packageName.startsWith('d3-')) {
    return 'sb-app-charts'
  }

  if (
    packageName === 'react-markdown' ||
    packageName === 'react-syntax-highlighter' ||
    packageName === 'remark-gfm' ||
    packageName.startsWith('remark-') ||
    packageName.startsWith('rehype-') ||
    packageName.startsWith('micromark') ||
    packageName.startsWith('mdast-') ||
    packageName.startsWith('hast-')
  ) {
    return 'sb-app-markdown'
  }

  if (packageName === 'axe-core' || packageName === '@storybook/addon-a11y') {
    return 'sb-addon-a11y'
  }

  if (
    packageName === '@storybook/addon-docs' ||
    packageName === 'markdown-to-jsx' ||
    packageName.startsWith('@mdx-js/')
  ) {
    return 'sb-addon-docs'
  }

  if (packageName.startsWith('@storybook/') || packageName === 'storybook') {
    return `sb-${sanitizeChunkName(packageName)}`
  }

  return undefined
}

type ManualChunks =
  | Record<string, string[]>
  | ((id: string, meta: ManualChunkMeta) => string | void)
  | undefined

function createManualChunks(existingManualChunks: ManualChunks) {
  return (id: string, meta: ManualChunkMeta): string | void => {
    const storybookChunk = resolveStorybookChunk(id)
    if (storybookChunk) {
      return storybookChunk
    }

    if (typeof existingManualChunks === 'function') {
      return existingManualChunks(id, meta)
    }

    return undefined
  }
}

function resolveOutputOptions(config: InlineConfig): OutputOptions {
  const currentOutput = config.build?.rollupOptions?.output
  if (Array.isArray(currentOutput)) {
    return {}
  }

  const output = currentOutput ?? {}
  return {
    ...output,
    manualChunks: createManualChunks(output.manualChunks),
  }
}

export function applyStorybookViteConfig(
  config: InlineConfig,
  pkgVersion: string,
  chunkSizeWarningLimit = 700,
): InlineConfig {
  return mergeConfig(config, {
    define: {
      __APP_VERSION__: JSON.stringify(`v${pkgVersion}`),
    },
    build: {
      chunkSizeWarningLimit,
      rollupOptions: {
        output: resolveOutputOptions(config),
      },
    },
  })
}
