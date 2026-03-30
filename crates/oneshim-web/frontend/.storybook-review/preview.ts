import type { Preview } from '@storybook/react'
import { withThemeByClassName } from '@storybook/addon-themes'

import '../src/i18n'
import '../src/index.css'

const preview: Preview = {
  decorators: [
    withThemeByClassName({
      themes: {
        light: 'light',
        dark: 'dark',
      },
      defaultTheme: 'light',
    }),
  ],
  parameters: {
    backgrounds: { disable: true },
  },
}

export default preview
