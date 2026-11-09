// SPDX-License-Identifier: GPL-3.0-or-later

export type Entry = {
  time: string
  path: string | null
  url: string | null
  deletion_url: string | null
}
export type Item = { entry: Entry; exists: boolean }
export type Day = { label: string; items: Item[] }

/** Identical lines in the file are one capture, so this is unique once `unique` has run. */
export function entryKey(entry: Entry): string {
  return JSON.stringify([entry.time, entry.path, entry.url, entry.deletion_url])
}

export function unique(items: Item[]): Item[] {
  return [...new Map(items.map((item) => [entryKey(item.entry), item])).values()]
}

export function fileName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1)
}

export function title(entry: Entry): string {
  if (entry.path) return fileName(entry.path)
  if (!entry.url) return entry.time
  try {
    return new URL(entry.url).host
  } catch {
    return entry.url
  }
}

/** Every word has to appear somewhere, in any order, ignoring case. */
export function matches(entry: Entry, query: string): boolean {
  const haystack = [entry.path, entry.url, entry.time].filter(Boolean).join(' ').toLowerCase()
  return query.toLowerCase().split(/\s+/).every((word) => haystack.includes(word))
}

export function clock(time: string): string {
  const date = new Date(time)
  if (isNaN(date.getTime())) return time
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function dayLabel(date: Date, today: Date): string {
  const days = Math.round((startOfDay(today) - startOfDay(date)) / 86_400_000)
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  return date.toLocaleDateString('en', {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    year: date.getFullYear() === today.getFullYear() ? undefined : 'numeric',
  })
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
}

/** Keeps the order it gets. A damaged time lands in a group of its own instead of hiding the capture,
 * and it cannot split the day around it. */
export function byDay(items: Item[], today = new Date()): Day[] {
  const days = new Map<string, Item[]>()
  for (const item of items) {
    const date = new Date(item.entry.time)
    const label = isNaN(date.getTime()) ? 'Unknown date' : dayLabel(date, today)
    days.set(label, [...(days.get(label) ?? []), item])
  }
  return [...days].map(([label, items]) => ({ label, items }))
}

export type Pick = 'one' | 'toggle' | 'range'

export function pick(selected: ReadonlySet<string>, order: string[], key: string, anchor: string | null, how: Pick): Set<string> {
  if (how === 'toggle') {
    const next = new Set(selected)
    if (!next.delete(key)) next.add(key)
    return next
  }
  const from = anchor === null ? -1 : order.indexOf(anchor)
  const to = order.indexOf(key)
  if (how === 'one' || from < 0 || to < 0) return new Set([key])
  return new Set([...selected, ...order.slice(Math.min(from, to), Math.max(from, to) + 1)])
}
