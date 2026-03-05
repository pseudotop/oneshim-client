/** @type {import('tailwindcss').Config} */

function withAlpha(varName) {
  return `rgb(var(${varName}) / <alpha-value>)`
}

export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
    "./.storybook/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        /* Keep the original primary scale for backward-compat */
        primary: {
          50: '#f0fdfa',
          100: '#ccfbf1',
          200: '#99f6e4',
          300: '#5eead4',
          400: '#2dd4bf',
          500: '#14b8a6',
          600: '#0d9488',
          700: '#0f766e',
          800: '#115e59',
          900: '#134e4a',
        },

        /* === Semantic design tokens === */
        brand: {
          DEFAULT: withAlpha('--brand'),
          hover: withAlpha('--brand-hover'),
          text: withAlpha('--brand-text'),
          signal: withAlpha('--brand-signal'),
          bar: withAlpha('--brand-bar'),
        },
        surface: {
          base: withAlpha('--surface-base'),
          elevated: withAlpha('--surface-elevated'),
          muted: withAlpha('--surface-muted'),
          inset: withAlpha('--surface-inset'),
          sunken: withAlpha('--surface-sunken'),
          overlay: withAlpha('--surface-overlay'),
        },
        content: {
          DEFAULT: withAlpha('--content'),
          secondary: withAlpha('--content-secondary'),
          tertiary: withAlpha('--content-tertiary'),
          inverse: withAlpha('--content-inverse'),
          muted: withAlpha('--content-muted'),
          strong: withAlpha('--content-strong'),
        },
        accent: {
          teal: withAlpha('--accent-teal'),
          blue: withAlpha('--accent-blue'),
          purple: withAlpha('--accent-purple'),
          green: withAlpha('--accent-green'),
          emerald: withAlpha('--accent-emerald'),
          amber: withAlpha('--accent-amber'),
          red: withAlpha('--accent-red'),
          orange: withAlpha('--accent-orange'),
          slate: withAlpha('--accent-slate'),
        },
        semantic: {
          success: withAlpha('--semantic-success'),
          warning: withAlpha('--semantic-warning'),
          error: withAlpha('--semantic-error'),
          info: withAlpha('--semantic-info'),
        },
        status: {
          connected: withAlpha('--status-connected'),
          connecting: withAlpha('--status-connecting'),
          disconnected: withAlpha('--status-disconnected'),
          error: withAlpha('--status-error'),
        },
        hover: withAlpha('--hover'),
        active: withAlpha('--active'),
        border: withAlpha('--border'),
      },
      borderColor: {
        DEFAULT: withAlpha('--border'),
        muted: withAlpha('--border-muted'),
        strong: withAlpha('--border-strong'),
      },
    },
  },
  plugins: [],
}
