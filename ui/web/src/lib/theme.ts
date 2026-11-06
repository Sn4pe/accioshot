// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from '@tauri-apps/api/core'

export async function applyTheme(): Promise<void> {
  document.documentElement.dataset.theme = await invoke<string>('ui_theme')
}
