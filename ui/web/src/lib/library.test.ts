// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import { byDay, matches, pick, title, unique, type Entry, type Item } from './library'

const entry = (over: Partial<Entry>): Entry => ({
  time: '2026-11-06T10:00:00+01:00',
  path: null,
  url: null,
  deletion_url: null,
  ...over,
})
const item = (time: string): Item => ({ entry: entry({ time }), exists: true })

describe('unique', () => {
  it('folds identical lines and keeps the rest in order', () => {
    const a = item('2026-11-06T10:00:00+01:00')
    const b = item('2026-11-06T09:00:00+01:00')
    expect(unique([a, b, { ...a }])).toEqual([a, b])
  })
})

describe('title', () => {
  it('prefers the file, then the host, then the time', () => {
    expect(title(entry({ path: '/a/b.png', url: 'https://x.io/1' }))).toBe('b.png')
    expect(title(entry({ url: 'https://x.io/1' }))).toBe('x.io')
    expect(title(entry({ url: 'not a url' }))).toBe('not a url')
    expect(title(entry({ time: 'cut' }))).toBe('cut')
  })
})

describe('matches', () => {
  const capture = entry({ path: '/shots/Invoice_May.png', url: 'https://x.io/abc' })

  it('finds every word anywhere, ignoring case', () => {
    expect(matches(capture, '')).toBe(true)
    expect(matches(capture, 'invoice')).toBe(true)
    expect(matches(capture, 'abc  may')).toBe(true)
    expect(matches(capture, '2026-11')).toBe(true)
    expect(matches(capture, 'invoice june')).toBe(false)
  })

  it('does not trip on what a capture lacks', () => {
    expect(matches(entry({}), 'null')).toBe(false)
  })
})

describe('byDay', () => {
  const today = new Date(2026, 10, 6, 12)

  it('names recent days and keeps the order', () => {
    const days = byDay(
      [
        item(new Date(2026, 10, 6, 11).toISOString()),
        item(new Date(2026, 10, 6, 9).toISOString()),
        item(new Date(2026, 10, 5, 23).toISOString()),
      ],
      today,
    )
    expect(days.map((day) => [day.label, day.items.length])).toEqual([
      ['Today', 2],
      ['Yesterday', 1],
    ])
  })

  it('shows the year only for other years', () => {
    const [thisYear, lastYear] = byDay(
      [item(new Date(2026, 0, 2).toISOString()), item(new Date(2025, 0, 2).toISOString())],
      today,
    )
    expect(thisYear.label).not.toContain('2026')
    expect(lastYear.label).toContain('2025')
  })

  it('keeps captures with a damaged time without splitting the day', () => {
    const now = new Date(2026, 10, 6, 11).toISOString()
    const days = byDay([item(now), item('cut'), item(now)], today)
    expect(days.map((day) => [day.label, day.items.length])).toEqual([
      ['Today', 2],
      ['Unknown date', 1],
    ])
  })
})

describe('pick', () => {
  const order = ['a', 'b', 'c', 'd']

  it('selects one, toggles, and ranges from the anchor either way', () => {
    expect([...pick(new Set(['a', 'b']), order, 'c', 'a', 'one')]).toEqual(['c'])
    expect([...pick(new Set(['a']), order, 'c', 'a', 'toggle')].sort()).toEqual(['a', 'c'])
    expect([...pick(new Set(['a', 'c']), order, 'c', 'a', 'toggle')]).toEqual(['a'])
    expect([...pick(new Set(['d']), order, 'b', 'd', 'range')].sort()).toEqual(['b', 'c', 'd'])
    expect([...pick(new Set(), order, 'b', null, 'range')]).toEqual(['b'])
  })
})
