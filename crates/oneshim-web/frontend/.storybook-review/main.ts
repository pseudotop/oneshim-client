import type { StorybookConfig } from '@storybook/react-vite'
import { readFileSync } from 'node:fs'
import { applyStorybookViteConfig } from '../.storybook/vite-shared.ts'

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'))

const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  addons: ['@storybook/addon-themes'],
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  viteFinal(config) {
    return applyStorybookViteConfig(config, pkg.version, 900)
  },
}

export default config
