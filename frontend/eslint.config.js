import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  },
  {
    // Transport-isolation rule: UI code depends only on `RepoClient`, never on a
    // concrete transport. Only files under `src/ipc/` (the transport implementations
    // themselves) may import `@tauri-apps/*` or `vscode` directly — scoped to all of `src/`
    // so any future directory (not just today's `src/components`/`src/state`) is covered
    // by construction.
    files: ['src/**/*.{ts,tsx}'],
    ignores: ['src/ipc/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@tauri-apps/*'],
              message: 'UI code must go through frontend/src/ipc/RepoClient.',
            },
            {
              group: ['vscode'],
              message: 'UI code must go through frontend/src/ipc/RepoClient.',
            },
          ],
        },
      ],
    },
  },
])
