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

Promise.all([invoke<keyof typeof WINDOWS>('window_name'), applyTheme()]).then(([name]) => {
  mount(WINDOWS[name], { target: document.getElementById('app')! })
})
