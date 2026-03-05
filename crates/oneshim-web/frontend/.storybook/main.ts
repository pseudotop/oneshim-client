import type { StorybookConfig } from '@storybook/react-vite'
import { readFileSync } from 'node:fs'

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'))

const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(ts|tsx)'],
  addons: [
    '@storybook/addon-a11y',
    '@storybook/addon-themes',
  ],
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  viteFinal(config) {
    config.define = {
      ...config.define,
      __APP_VERSION__: JSON.stringify(`v${pkg.version}`),
    }
    return config
  },
}

export default config
