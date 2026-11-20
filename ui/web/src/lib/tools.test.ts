// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import type { Annotation, Text } from './model'
import { cursorFor, defaultStyle, isTool, longEnough, restyled, type EditorSettings } from './tools'

const settings: EditorSettings = {
  tool: 'Arrow',
  colors: ['#ff0000', '#00ff00'],
  highlight_color: '#ffff00',
  stroke_width: 4,
  font_family: 'sans-serif',
  font_size: 24,
  step_size: 32,
  blur_strength: 8,
  pixel_size: 12,
  remember_last: false,
  initial_zoom: 'fit',
  zoom_step: 1.25,
  max_zoom: 8,
  keys: {},
}
const line: Annotation = { id: 1, kind: 'Line', a: { x: 0, y: 0 }, b: { x: 3, y: 4 }, color: '#000', size: 2 }
const text: Text = {
  id: 2,
  kind: 'Text',
  at: { x: 0, y: 0 },
  text: 'hi',
  color: '#000',
  size: 10,
  font: 'sans-serif',
  width: 1,
  height: 1,
}
const measure = (value: string, size: number) => ({ width: value.length * size, height: size })

describe('tools', () => {
  it('gives each tool one cursor', () => {
    expect(cursorFor('Select')).toBe('default')
    expect(cursorFor('Text')).toBe('text')
    expect(cursorFor('Arrow')).toBe('crosshair')
    expect(cursorFor('Crop')).toBe('crosshair')
  })

  it('knows its tools', () => {
    expect(isTool('Blur')).toBe(true)
    expect(isTool('Brush')).toBe(false)
    expect(isTool(undefined)).toBe(false)
  })

  it('starts each tool from the settings', () => {
    expect(defaultStyle(settings, 'Highlight')).toEqual({ color: '#ffff00', size: 0 })
    expect(defaultStyle(settings, 'Text')).toEqual({ color: '#ff0000', size: 24 })
    expect(defaultStyle(settings, 'Pixelate')).toEqual({ color: '', size: 12 })
    expect(defaultStyle(settings, 'Arrow')).toEqual({ color: '#ff0000', size: 4 })
  })

  it('drops shapes shorter than the minimum', () => {
    expect(longEnough(line, 5)).toBe(true)
    expect(longEnough(line, 6)).toBe(false)
  })

  it('sizes only the kind the slider is for', () => {
    expect(restyled(line, { color: '#fff', size: 9 }, 'Text', measure)).toMatchObject({ color: '#fff', size: 2 })
    expect(restyled(line, { size: 9 }, 'Line', measure)).toMatchObject({ size: 9 })
  })

  it('measures text again after a new size', () => {
    expect(restyled(text, { size: 20 }, 'Text', measure)).toMatchObject({ size: 20, width: 40, height: 20 })
  })
})
