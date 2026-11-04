/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { thirdPartyNoticesPlugin } from './vite.notices.ts'
import { excludeGrammarsPlugin } from './vite.grammars.ts'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), thirdPartyNoticesPlugin(), excludeGrammarsPlugin()],
  test: {
    environment: 'jsdom',
    globals: true,
    passWithNoTests: true,
    setupFiles: ['./src/setupTests.ts'],
  },
})
