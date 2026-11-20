// SPDX-License-Identifier: GPL-3.0-or-later
import type { Tool } from './model'

export function cursorFor(tool: Tool): string {
  if (tool === 'Select') return 'default'
  return tool === 'Text' ? 'text' : 'crosshair'
}
