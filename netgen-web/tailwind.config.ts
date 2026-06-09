import type { Config } from 'tailwindcss'

const config: Config = {
  content: [
    './app/**/*.{ts,tsx}',
    './src/**/*.{ts,tsx}',
    './components/**/*.{ts,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        bg: {
          page:    '#f5f4f1',
          surface: '#ffffff',
          canvas:  '#fafaf8',
          hover:   '#f0ede6',
          input:   '#f5f4f1',
        },
        border: {
          subtle:  '#e8e6e0',
          default: '#d8d6cc',
          strong:  '#c8c6bc',
        },
        'text-primary':   '#1a1a18',
        'text-secondary': '#5f5e5a',
        'text-tertiary':  '#888780',
        'text-disabled':  '#b4b2a9',
        accent: {
          50:  '#edf4fd',
          100: '#b5d4f4',
          400: '#378add',
          600: '#185fa5',
          800: '#0c447c',
        },
      },
      fontFamily: {
        sans: ['var(--font-inter)', '-apple-system', 'BlinkMacSystemFont', 'sans-serif'],
        mono: ['var(--font-mono)', '"Fira Code"', '"Cascadia Code"', 'monospace'],
      },
      borderRadius: {
        sm:   '5px',
        md:   '7px',
        lg:   '9px',
        xl:   '12px',
        full: '9999px',
      },
      fontSize: {
        '2xs': '10px',
        xs:    '11px',
        sm:    '12px',
        base:  '13px',
        md:    '14px',
      },
    },
  },
  plugins: [],
}

export default config
