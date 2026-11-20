// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import { cursorFor } from './tools'

describe('tools', () => {
  it('gives each tool one cursor', () => {
    expect(cursorFor('Select')).toBe('default')
    expect(cursorFor('Text')).toBe('text')
    expect(cursorFor('Arrow')).toBe('crosshair')
    expect(cursorFor('Crop')).toBe('crosshair')
  })
})
