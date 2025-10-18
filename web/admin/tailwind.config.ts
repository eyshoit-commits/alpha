import type { Config } from "tailwindcss";

const config: Config = {
  content: [
    "./src/**/*.{js,ts,jsx,tsx}",
    "./src/components/**/*.{js,ts,jsx,tsx}",
  ],
  safelist: [
    'bg-blue-600',
    'bg-blue-500',
    'bg-blue-50',
    'bg-blue-100',
    'hover:bg-blue-500',
    'hover:bg-blue-50',
    'hover:border-blue-400',
    'border-blue-300',
    'border-blue-400',
    'border-blue-500',
    'border-blue-200',
    'border-blue-100',
    'text-blue-600',
    'text-blue-700',
    'text-blue-900',
    'ring-blue-400',
    'focus:ring-blue-400',
    'focus:border-blue-500',
    'from-blue-50',
    'to-white',
  ],
  theme: {
    extend: {
      colors: {
        cyber: {
          bg: '#0a0e27',
          card: '#12172f',
          dark: '#1a1f3a',
          border: '#2d3561',
        },
        neon: {
          orange: '#EC5800',
          purple: '#AF75FF',
          mint: '#75ffaf',
        },
        blue: {
          50: '#eff6ff',
          100: '#dbeafe',
          200: '#bfdbfe',
          300: '#93c5fd',
          400: '#60a5fa',
          500: '#3b82f6',
          600: '#2563eb',
          700: '#1d4ed8',
          800: '#1e40af',
          900: '#1e3a8a',
        },
      },
      boxShadow: {
        'neon-blue': '0 0 20px rgba(0, 212, 255, 0.5)',
        'neon-cyan': '0 0 20px rgba(0, 255, 245, 0.5)',
      },
      backgroundImage: {
        'cyber-grid': 'linear-gradient(rgba(0, 212, 255, 0.05) 1px, transparent 1px), linear-gradient(90deg, rgba(0, 212, 255, 0.05) 1px, transparent 1px)',
      },
      backgroundSize: {
        'grid': '50px 50px',
      },
    },
  },
  plugins: [],
};

export default config;
