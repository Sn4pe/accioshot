// SPDX-License-Identifier: GPL-3.0-or-later
import { Channel, invoke } from '@tauri-apps/api/core'
import type { Entry, Item } from './lib/library'
import type { EditorSettings, SavedState } from './lib/tools'

export type Config = {
  save_dir?: string | null
  uploader?: string | null
  after_capture: string[]
  after_upload: string[]
  notifications: boolean
  delay: number
  name_pattern: string
  region: { snap_windows: boolean; magnifier: boolean; magnifier_size: number; magnifier_pixels: number }
  editor: { command: string }
  tray: { region_hotkey: string }
  history: { thumbnail_width: number; thumbnail_height: number; max_items: number; filter_missing_files: boolean }
  ui: { theme: string }
}
export type Preferences = { config: Config; uploaders: string[]; has_hotkey: boolean }
export type HistoryItems = { items: Item[]; thumbnail_width: number; thumbnail_height: number }
export type Part = 'image' | 'path' | 'url' | 'deletion_url' | 'file' | 'folder'

export const windowName = () => invoke<string>('window_name')

export async function applyTheme(): Promise<void> {
  document.documentElement.dataset.theme = await invoke<string>('ui_theme')
}

export const editorSettings = () => invoke<EditorSettings>('settings')
export const loadState = () => invoke<SavedState | null>('load_state')
export const saveState = (state: SavedState) => invoke<void>('save_state', { state })
export const readImage = () => invoke<ArrayBuffer>('read_image')
export const finishImage = (command: 'save' | 'copy', png: Uint8Array) => invoke<void>(command, png)
export const discard = (loaded: boolean) => invoke<void>('discard', { loaded })

export const historyItems = () => invoke<HistoryItems>('history_items')
export const thumbnail = (entry: Entry) => invoke<ArrayBuffer>('thumbnail', { entry })
export const image = (entry: Entry) => invoke<ArrayBuffer>('image', { entry })
export const copyEntry = (entry: Entry, part: Part) => invoke<void>('copy_entry', { entry, part })
export const openEntry = (entry: Entry, part: Part) => invoke<void>('open_entry', { entry, part })
export const editEntry = (entry: Entry) => invoke<void>('edit_entry', { entry })
export const deleteEntries = (entries: Entry[], deleteFile: boolean) =>
  invoke<string[]>('delete_entries', { entries, deleteFile })
export const startDrag = (paths: string[], icon: string) =>
  invoke<void>('plugin:drag|start_drag', {
    item: paths,
    image: icon,
    options: { mode: 'copy' },
    onEvent: new Channel(),
  })

export const loadPreferences = () => invoke<Preferences>('load_preferences')
export const savePreferences = (config: Config) => invoke<void>('save_preferences', { config })
export const openConfig = () => invoke<void>('open_config')
