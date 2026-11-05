/// <reference types="vitest/config" />
import { defineConfig, searchForWorkspaceRoot } from 'vite'
import react from '@vitejs/plugin-react'
import { thirdPartyNoticesPlugin } from './vite.notices.ts'
import { excludeGrammarsPlugin } from './vite.grammars.ts'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), thirdPartyNoticesPlugin(), excludeGrammarsPlugin()],
  server: {
    fs: {
      // Vite's default (this folder) plus the user guide, which src/lib/searchSyntax.test.ts
      // imports with `?raw` to keep the guide's search table in step with the app (#320).
      allow: [searchForWorkspaceRoot(process.cwd()), '../docs/guide'],
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    passWithNoTests: true,
    setupFiles: ['./src/setupTests.ts'],
  },
})
