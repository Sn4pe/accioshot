<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Channel, invoke } from '@tauri-apps/api/core'
  import { onMount, tick } from 'svelte'
  import Frame from './Frame.svelte'
  import Viewer from './Viewer.svelte'
  import { byDay, entryKey, fileName, matches, pick, title, unique, type Entry, type Item } from './lib/library'

  type Part = 'image' | 'path' | 'url' | 'deletion_url' | 'file' | 'folder'

  const NOTE_MS = 2500

  let items = $state.raw<Item[] | null>(null)
  let width = $state(250)
  let height = $state(150)
  let query = $state('')
  let selected = $state.raw<Set<string>>(new Set())
  let anchor: string | null = null
  let confirming = $state(false)
  let note = $state('')
  let failure = $state('')
  let noteTimer: ReturnType<typeof setTimeout> | undefined
  let revision = $state(0)
  let viewing = $state<'preview' | 'full' | null>(null)
  let swallowSpace = false

  const shown = $derived((items ?? []).filter((item) => matches(item.entry, query.trim())))
  const days = $derived(byDay(shown))
  const chosen = $derived((items ?? []).filter((item) => selected.has(entryKey(item.entry))))
  const current = $derived(chosen.length === 1 ? chosen[0] : null)
  const order = $derived(days.flatMap((day) => day.items))
  const position = $derived(current ? order.findIndex((item) => same(item.entry, current.entry)) : -1)
  const missing = $derived((items ?? []).filter((item) => item.entry.path && !item.exists))

  function same(a: Entry, b: Entry): boolean {
    return entryKey(a) === entryKey(b)
  }

  async function refresh() {
    try {
      const result = await invoke<{ items: Item[]; thumbnail_width: number; thumbnail_height: number }>('history_items')
      items = unique(result.items)
      width = result.thumbnail_width
      height = result.thumbnail_height
    } catch (error) {
      failure = String(error)
    }
  }

  function say(text: string) {
    note = text
    failure = ''
    clearTimeout(noteTimer)
    noteTimer = setTimeout(() => (note = ''), NOTE_MS)
  }

  async function copy(part: Part, done: string) {
    if (!current) return
    try {
      await invoke('copy_entry', { entry: current.entry, part })
      say(done)
    } catch (error) {
      failure = String(error)
    }
  }

  async function open(part: Part) {
    if (!current) return
    try {
      await invoke('open_entry', { entry: current.entry, part })
    } catch (error) {
      failure = String(error)
    }
  }

  async function edit() {
    if (!current) return
    try {
      await invoke('edit_entry', { entry: current.entry })
      revision++
      await refresh()
    } catch (error) {
      failure = String(error)
    }
  }

  async function drag(item: Item, icon: string) {
    const group = selected.has(entryKey(item.entry)) ? chosen : [item]
    const paths = group.filter((one) => one.exists).map((one) => one.entry.path!)
    try {
      await invoke('plugin:drag|start_drag', { item: paths, image: icon, options: { mode: 'copy' }, onEvent: new Channel() })
    } catch (error) {
      failure = String(error)
    }
  }

  $effect(() => {
    if (!current) viewing = null
  })

  function view(item: Item) {
    selected = new Set([entryKey(item.entry)])
    anchor = entryKey(item.entry)
    viewing = 'full'
  }

  function step(by: number) {
    const next = order[position < 0 ? 0 : Math.min(Math.max(position + by, 0), order.length - 1)]
    if (!next) return
    selected = new Set([entryKey(next.entry)])
    anchor = entryKey(next.entry)
    confirming = false
    tick().then(() => document.querySelector('.frame.selected')?.scrollIntoView({ block: 'nearest' }))
  }

  async function remove(deleteFile: boolean) {
    if (!chosen.length) return
    const entries = chosen.map((item) => item.entry)
    const next = current && (order[position + 1] ?? order[position - 1])
    confirming = false
    try {
      const failures = await invoke<string[]>('delete_entries', { entries, deleteFile })
      selected = new Set()
      await refresh()
      if (viewing && next) selected = new Set([entryKey(next.entry)])
      if (failures.length) failure = failures.join('\n')
      else say(deleteFile ? 'Deleted' : 'Removed from history')
    } catch (error) {
      failure = String(error)
    }
  }

  function select(entry: Entry, event: MouseEvent) {
    const key = entryKey(entry)
    const how = event.metaKey || event.ctrlKey ? 'toggle' : event.shiftKey ? 'range' : 'one'
    const keys = order.map((item) => entryKey(item.entry))
    selected = how === 'one' && current && same(current.entry, entry) ? new Set() : pick(selected, keys, key, anchor, how)
    if (how !== 'range' || anchor === null) anchor = key
    confirming = false
    failure = ''
  }

  function cleanUp() {
    selected = new Set(missing.map((item) => entryKey(item.entry)))
    confirming = true
    failure = ''
  }

  function onKeyDown(event: KeyboardEvent) {
    swallowSpace = false
    if (event.key === ' ' && event.target instanceof HTMLInputElement) return
    if (event.key === ' ' && event.repeat) {
      event.preventDefault()
      return
    }
    if (viewing) {
      if (event.key === 'Escape' && !confirming) {
        viewing = null
        return
      }
      if (event.key === ' ') {
        event.preventDefault()
        swallowSpace = true
        if (viewing === 'preview') viewing = null
        return
      }
    }
    if (event.key === 'Escape') {
      if (confirming) confirming = false
      else if (query) query = ''
      else selected = new Set()
      return
    }
    if (event.target instanceof HTMLInputElement) return
    if ((event.ctrlKey || event.metaKey) && event.key === 'a') {
      event.preventDefault()
      selected = new Set(shown.map((item) => entryKey(item.entry)))
      return
    }
    if (chosen.length && (event.key === 'Delete' || event.key === 'Backspace')) confirming = true
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault()
      step(event.key === 'ArrowLeft' ? -1 : 1)
      return
    }
    if (!current) return
    if (event.key === ' ') {
      event.preventDefault()
      swallowSpace = true
      viewing = 'preview'
      return
    }
    else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      if (current.exists) open('file')
      else if (current.entry.url) open('url')
    }
    else if ((event.ctrlKey || event.metaKey) && event.key === 'c') {
      if (current.exists) copy('image', 'Image copied')
      else if (current.entry.url) copy('url', 'URL copied')
    }
  }

  onMount(() => {
    refresh()
    // New captures show up without reopening the window.
    window.addEventListener('focus', refresh)
    return () => {
      window.removeEventListener('focus', refresh)
      clearTimeout(noteTimer)
    }
  })
</script>

<svelte:window
  onkeydown={onKeyDown}
  onkeyup={(event) => event.key === ' ' && swallowSpace && event.preventDefault()}
/>

<div class="history">
  <header>
    <input type="search" placeholder="Search names, links and dates" aria-label="Search" bind:value={query} />
    {#if items}
      {#if missing.length}
        <button class="quiet" onclick={cleanUp}>Clean up {missing.length} missing</button>
      {/if}
      <span class="count">{query.trim() ? `${shown.length} of ${items.length}` : items.length}</span>
    {/if}
  </header>

  <div class="body">
    <main>
      {#if items && !items.length}
        <p class="empty">No captures yet.</p>
      {:else if items && !shown.length}
        <p class="empty">Nothing matches.</p>
      {/if}
      {#each days as day}
        <section>
          <h2>{day.label}</h2>
          <div class="sheet" style:grid-template-columns="repeat(auto-fill, {width + 12}px)">
            {#each day.items as item (`${revision}:${entryKey(item.entry)}`)}
              <Frame
                {item}
                {width}
                {height}
                selected={selected.has(entryKey(item.entry))}
                onselect={(event) => select(item.entry, event)}
                onopen={() => view(item)}
                ondrag={(icon) => drag(item, icon)}
              />
            {/each}
          </div>
        </section>
      {/each}
    </main>
    {#if viewing && current}
      <Viewer
        item={current}
        {revision}
        position={position + 1}
        total={order.length}
        preview={viewing === 'preview'}
        onclose={() => (viewing = null)}
      />
    {/if}
  </div>

  {#if failure}
    <p class="failure" role="alert">{failure}</p>
  {/if}

  {#if current}
    {@const entry = current.entry}
    <footer>
      <div class="detail">
        <strong>{entry.path ? fileName(entry.path) : title(entry)}</strong>
        <span class="where">{entry.url ?? entry.path}</span>
      </div>
      <div class="actions">
        {#if confirming}
          <span class="question">Remove this capture?</span>
          <button class="quiet" onclick={() => (confirming = false)}>Keep</button>
          <button class="quiet" onclick={() => remove(false)}>From history only</button>
          {#if current.exists}
            <button class="primary danger" onclick={() => remove(true)}>Delete the file too</button>
          {/if}
        {:else}
          <span class="said" aria-live="polite">{note}</span>
          {#if current.exists}
            <button class="quiet" onclick={() => copy('image', 'Image copied')}>Copy image</button>
            <button class="quiet" onclick={() => copy('path', 'Path copied')}>Copy path</button>
            <button class="quiet" onclick={edit}>Edit</button>
            <button class="quiet" onclick={() => open('file')}>Open</button>
            <button class="quiet" onclick={() => open('folder')}>Folder</button>
          {/if}
          {#if entry.url}
            <button class="quiet" onclick={() => copy('url', 'URL copied')}>Copy URL</button>
            <button class="quiet" onclick={() => open('url')}>Browser</button>
          {/if}
          {#if entry.deletion_url}
            <button class="quiet" title="Opens the host's deletion page" onclick={() => open('deletion_url')}>
              Delete upload
            </button>
          {/if}
          <button class="quiet" onclick={() => (confirming = true)}>Remove</button>
        {/if}
      </div>
    </footer>
  {:else if chosen.length > 1}
    <footer>
      <div class="detail">
        <strong>{chosen.length} selected</strong>
      </div>
      <div class="actions">
        {#if confirming}
          <span class="question">Remove {chosen.length} captures?</span>
          <button class="quiet" onclick={() => (confirming = false)}>Keep</button>
          <button class="quiet" onclick={() => remove(false)}>From history only</button>
          {#if chosen.some((item) => item.exists)}
            <button class="primary danger" onclick={() => remove(true)}>Delete the files too</button>
          {/if}
        {:else}
          <span class="said" aria-live="polite">{note}</span>
          <button class="quiet" onclick={() => (confirming = true)}>Remove</button>
        {/if}
      </div>
    </footer>
  {:else if note}
    <footer><span class="said" aria-live="polite">{note}</span></footer>
  {/if}
</div>

<style>
  .history {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 12px;
    background: var(--bar);
  }
  input {
    flex: 1;
    max-width: 420px;
    height: 32px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 0;
    border-radius: 4px;
    background: var(--bg);
    color: inherit;
    font: inherit;
  }
  input::placeholder {
    color: var(--faint);
  }
  .count {
    margin-left: auto;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 16px 24px;
    scrollbar-color: var(--active) transparent;
  }
  h2 {
    position: sticky;
    top: 0;
    z-index: 1;
    margin: 0;
    padding: 14px 6px 8px;
    background: var(--bg);
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }
  .sheet {
    display: grid;
    gap: 6px;
  }
  .empty {
    margin: 48px 6px;
    color: var(--muted);
  }
  .failure {
    margin: 0;
    padding: 10px 16px;
    background: var(--failure-bg);
    color: var(--failure-text);
    user-select: text;
    -webkit-user-select: text;
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 20px;
    min-height: 32px;
    padding: 8px 12px 8px 16px;
    background: var(--bar);
  }
  .detail {
    flex: 1 1 200px;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .detail strong,
  .where {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail strong {
    font-weight: 600;
  }
  .where {
    color: var(--muted);
    user-select: text;
    -webkit-user-select: text;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .said {
    margin-right: 8px;
    color: var(--soft);
  }
  .question {
    margin-right: 8px;
  }
</style>
