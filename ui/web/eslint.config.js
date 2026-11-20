// SPDX-License-Identifier: GPL-3.0-or-later
import js from '@eslint/js'
import svelte from 'eslint-plugin-svelte'
import globals from 'globals'
import ts from 'typescript-eslint'
import svelteConfig from './svelte.config.js'

const ROOT_CAUSE = 'Fix the root cause instead: derive the value, handle the event, or use an attachment.'

const timers = ['setTimeout', 'setInterval', 'requestAnimationFrame', 'queueMicrotask'].map((name) => ({
  name,
  message: `${name} hides an ordering problem. ${ROOT_CAUSE}`,
}))

const effects = [
  { selector: "CallExpression[callee.name='$effect']", message: `No $effect. ${ROOT_CAUSE}` },
  {
    selector: "CallExpression[callee.object.name='$effect'][callee.property.name='pre']",
    message: `No $effect.pre. ${ROOT_CAUSE}`,
  },
]

const untrack = {
  name: 'svelte',
  importNames: ['untrack'],
  message: 'Read the value outside the reactive scope instead.',
}
const core = { name: '@tauri-apps/api/core', message: 'Only src/ipc.ts imports @tauri-apps/api/core.' }
const pure = [
  { group: ['@tauri-apps/api', '@tauri-apps/api/*'], message: 'src/lib is pure, Tauri goes through src/ipc.ts.' },
  { group: ['*.svelte'], message: 'src/lib is pure, it imports no components.' },
]

export default ts.config(
  { ignores: ['dist/'] },
  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  {
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        projectService: { allowDefaultProject: ['*.js', '*.ts'] },
        tsconfigRootDir: import.meta.dirname,
        extraFileExtensions: ['.svelte'],
      },
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser, svelteConfig } },
  },
  {
    rules: {
      'svelte/prefer-writable-derived': 'error',
      'svelte/require-each-key': 'error',
      'svelte/no-dom-manipulating': 'error',
      'svelte/valid-compile': 'error',
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/consistent-type-assertions': ['error', { assertionStyle: 'never' }],
      '@typescript-eslint/no-empty-function': 'error',
      'no-empty': ['error', { allowEmptyCatch: false }],
      'no-restricted-globals': ['error', ...timers],
      'no-restricted-syntax': ['error', ...effects],
      'no-restricted-imports': ['error', { paths: [untrack, core] }],
    },
  },
  {
    files: ['src/lib/**'],
    rules: { 'no-restricted-imports': ['error', { paths: [untrack, core], patterns: pure }] },
  },
  {
    files: ['src/History.svelte'],
    rules: { 'no-restricted-globals': ['error', ...timers.filter(({ name }) => name !== 'setTimeout')] },
  },
  {
    files: ['src/ipc.ts'],
    rules: { 'no-restricted-imports': ['error', { paths: [untrack] }] },
  },
)
