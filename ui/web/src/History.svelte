<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { onMount } from 'svelte'
  import Frame from './Frame.svelte'
  import { byDay, entryKey, fileName, matches, title, unique, type Entry, type Item } from './lib/library'

  type Part = 'image' | 'path' | 'url' | 'deletion_url' | 'file' | 'folder'

  const NOTE_MS = 2500

  let items = $state.raw<Item[] | null>(null)
  let width = $state(250)
  let height = $state(150)
  let query = $state('')
  let selected = $state.raw<Entry | null>(null)
  let confirming = $state(false)
  let note = $state('')
  let failure = $state('')
  let noteTimer: ReturnType<typeof setTimeout> | undefined

  const shown = $derived((items ?? []).filter((item) => matches(item.entry, query.trim())))
  const days = $derived(byDay(shown))
  const current = $derived(selected && (items ?? []).find((item) => same(item.entry, selected!)))

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

  function openDefault(item: Item) {
    selected = item.entry
    if (item.exists) open('file')
    else if (item.entry.url) open('url')
  }

  async function remove(deleteFile: boolean) {
    if (!current) return
    const entry = current.entry
    try {
      await invoke('delete_entry', { entry, deleteFile })
      items = (items ?? []).filter((item) => !same(item.entry, entry))
      selected = null
      confirming = false
      say(deleteFile ? 'Deleted' : 'Removed from history')
    } catch (error) {
      confirming = false
      failure = String(error)
    }
  }

  function select(entry: Entry) {
    selected = selected && same(selected, entry) ? null : entry
    confirming = false
    failure = ''
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (confirming) confirming = false
      else if (query) query = ''
      else selected = null
      return
    }
    if (event.target instanceof HTMLInputElement || !current) return
    if (event.key === 'Delete') confirming = true
    else if (event.key === 'Enter' && event.ctrlKey) openDefault(current)
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

<svelte:window onkeydown={onKeyDown} />

<div class="history">
  <header>
    <input type="search" placeholder="Search names, links and dates" aria-label="Search" bind:value={query} />
    {#if items}
      <span class="count">{query.trim() ? `${shown.length} of ${items.length}` : items.length}</span>
    {/if}
  </header>

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
          {#each day.items as item (entryKey(item.entry))}
            <Frame
              {item}
              {width}
              {height}
              selected={selected !== null && same(selected, item.entry)}
              onselect={() => select(item.entry)}
              onopen={() => openDefault(item)}
            />
          {/each}
        </div>
      </section>
    {/each}
  </main>

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
