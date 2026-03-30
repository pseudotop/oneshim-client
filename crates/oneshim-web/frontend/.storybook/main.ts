import type { StorybookConfig } from '@storybook/react-vite'
import { readFileSync } from 'node:fs'
import { applyStorybookViteConfig } from './vite-shared.ts'

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'))

const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  addons: [
    '@storybook/addon-a11y',
    '@storybook/addon-themes',
    '@storybook/addon-docs',
  ],
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  viteFinal(config) {
    return applyStorybookViteConfig(config, pkg.version)
  },
}

export default config
