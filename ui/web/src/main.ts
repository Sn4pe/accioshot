import { invoke } from '@tauri-apps/api/core'
import { mount } from 'svelte'
import './app.css'
import Editor from './Editor.svelte'
import History from './History.svelte'
import Settings from './Settings.svelte'
import { applyTheme } from './lib/theme'

const WINDOWS = { edit: Editor, history: History, settings: Settings }

Promise.all([invoke<keyof typeof WINDOWS>('window_name'), applyTheme()]).then(([name]) => {
  mount(WINDOWS[name], { target: document.getElementById('app')! })
})
