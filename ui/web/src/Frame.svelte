<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { onDestroy } from 'svelte'
  import { clock, title, type Item } from './lib/library'

  let {
    item,
    width,
    height,
    selected,
    onselect,
    onopen,
    ondrag,
  }: {
    item: Item
    width: number
    height: number
    selected: boolean
    onselect: (event: MouseEvent) => void
    onopen: () => void
    ondrag: (icon: string) => void
  } = $props()

  let src = $state('')
  let failed = $state(false)
  const entry = $derived(item.entry)
  const note = $derived(
    !entry.path ? 'Uploaded only' : !item.exists ? 'File is gone' : failed ? 'Cannot read this file' : '',
  )

  /** A long history holds hundreds of big files, so only what scrolls into view gets decoded. */
  function whenVisible(node: HTMLElement) {
    const observer = new IntersectionObserver(
      (seen) => {
        if (seen.some((one) => one.isIntersecting)) {
          observer.disconnect()
          load()
        }
      },
      { rootMargin: '200px' },
    )
    observer.observe(node)
    return () => observer.disconnect()
  }

  const BLANK = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII='
  let blob: Blob | null = null

  function iconUrl(): Promise<string> {
    return new Promise((resolve) => {
      if (!blob) return resolve(BLANK)
      const reader = new FileReader()
      reader.onload = () => resolve(String(reader.result))
      reader.readAsDataURL(blob)
    })
  }

  async function drag(event: DragEvent) {
    event.preventDefault()
    ondrag(await iconUrl())
  }

  async function load() {
    if (!entry.path || !item.exists) return
    try {
      const bytes = await invoke<ArrayBuffer>('thumbnail', { entry })
      blob = new Blob([bytes], { type: 'image/png' })
      src = URL.createObjectURL(blob)
    } catch {
      failed = true
    }
  }

  onDestroy(() => {
    if (src) URL.revokeObjectURL(src)
  })
</script>

<button
  class="frame"
  class:selected
  aria-pressed={selected}
  onclick={onselect}
  ondblclick={onopen}
  draggable={item.exists}
  ondragstart={drag}
  {@attach whenVisible}
>
  <span class="plate" style:width="{width}px" style:height="{height}px">
    {#if src}
      <img {src} alt="" />
    {:else if note}
      <span class="note">{note}</span>
    {/if}
  </span>
  <span class="caption" style:width="{width}px">
    <span class="name">{title(entry)}</span>
    <time>{clock(entry.time)}</time>
  </span>
</button>

<style>
  .frame {
    display: block;
    padding: 6px;
    border-radius: 6px;
    text-align: left;
  }
  .frame:hover .plate {
    background: var(--plate-hover);
  }
  .frame:hover .name {
    color: var(--strong);
  }
  .frame.selected {
    background: var(--active);
  }
  .frame.selected .name {
    color: var(--strong);
  }
  .plate {
    display: grid;
    place-items: center;
    overflow: hidden;
    background: var(--plate);
    border-radius: 3px;
    transition: background-color 100ms;
  }
  img {
    display: block;
    max-width: 100%;
    max-height: 100%;
  }
  .note {
    padding: 0 12px;
    color: var(--faint);
    text-align: center;
  }
  .caption {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 2px 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--soft);
  }
  time {
    flex: none;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
</style>
