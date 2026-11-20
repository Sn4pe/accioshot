// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import { eventCombo, keymap, normalizeCombo } from './keys'

describe('keys', () => {
  it('orders modifiers the same way', () => {
    expect(normalizeCombo('Shift + Ctrl + Z')).toBe('ctrl+shift+z')
    expect(eventCombo({ key: 'Z', ctrlKey: true, metaKey: false, altKey: false, shiftKey: true })).toBe('ctrl+shift+z')
  })

  it('lets the user override', () => {
    const map = keymap({ save: 'ctrl+enter' })
    expect(map.get('ctrl+enter')).toBe('save')
    expect(map.get('ctrl+s')).toBeUndefined()
    expect(map.get('a')).toBe('Arrow')
  })

  it('rejects unknown actions', () => {
    expect(() => keymap({ nonsense: 'x' })).toThrow('nonsense')
  })

  it('copies with ctrl+c unless turned off', () => {
    expect(keymap({}).get('ctrl+c')).toBe('copy')
    expect(keymap({ copy: '' }).get('ctrl+c')).toBeUndefined()
  })

  it('accepts alternatives', () => {
    const map = keymap({})
    expect(map.get('ctrl+y')).toBe('redo')
    expect(map.get('ctrl+shift+z')).toBe('redo')
  })

  it('lets the user turn a key off', () => {
    expect(keymap({ Arrow: '' }).get('a')).toBeUndefined()
  })
})

describe('zoom and move keys', () => {
  it('parses the + key', () => {
    expect(normalizeCombo('ctrl++')).toBe('ctrl++')
    expect(normalizeCombo('Shift + Ctrl ++')).toBe('ctrl+shift++')
    expect(normalizeCombo('+')).toBe('+')
    expect(keymap({}).get('ctrl++')).toBe('zoom_in')
  })

  it('names the space bar', () => {
    const combo = eventCombo({ key: ' ', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false })
    expect(combo).toBe('space')
    expect(keymap({}).get(combo)).toBe('pan')
  })

  it('moves with and without Shift', () => {
    const map = keymap({})
    expect(map.get('arrowleft')).toBe('move_left')
    expect(map.get('shift+arrowleft')).toBe('move_left')
  })
})
