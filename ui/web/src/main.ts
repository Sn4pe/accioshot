// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from '@tauri-apps/api/core'
import { mount } from 'svelte'
import './app.css'
import Editor from './Editor.svelte'
import History from './History.svelte'
import Settings from './Settings.svelte'
import { applyTheme } from './lib/theme'

const WINDOWS = { edit: Editor, history: History, settings: Settings }

// WebKit's page menu (Back, Reload, Inspect) has no place in an app window. Text fields keep theirs
// for cut and paste, and dev builds keep it for the inspector.
if (!import.meta.env.DEV) {
  document.addEventListener('contextmenu', (event) => {
    if (!(event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement)) event.preventDefault()
  })
}

function isWindow(name: string): name is keyof typeof WINDOWS {
  return Object.hasOwn(WINDOWS, name)
}

const target = document.getElementById('app') ?? document.body

Promise.all([invoke<string>('window_name'), applyTheme()])
  .then(([name]) => {
    if (!isWindow(name)) throw new Error(`Unknown window: ${name}`)
    mount(WINDOWS[name], { target })
  })
  .catch((error: unknown) => {
    target.textContent = String(error)
  })
