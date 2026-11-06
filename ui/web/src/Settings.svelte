<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { onMount } from 'svelte'
  import { applyTheme } from './lib/theme'

  type Config = {
    save_dir?: string | null
    uploader?: string | null
    after_capture: string[]
    after_upload: string[]
    notifications: boolean
    delay: number
    name_pattern: string
    region: { snap_windows: boolean; magnifier: boolean; magnifier_size: number; magnifier_pixels: number }
    editor: { command: string }
    tray: { region_hotkey: string }
    history: { thumbnail_width: number; thumbnail_height: number; max_items: number; filter_missing_files: boolean }
    ui: { theme: string }
  }

  const AFTER_CAPTURE = [
    ['AnnotateImage', 'Open in the editor'],
    ['SaveImageToFile', 'Save to the folder'],
    ['CopyImageToClipboard', 'Copy the image'],
    ['UploadImageToHost', 'Upload'],
  ]
  const AFTER_UPLOAD = [
    ['CopyURLToClipboard', 'Copy the URL'],
    ['OpenURL', 'Open the URL'],
  ]
  const BUILTIN = 'builtin'
  const SAVED = 'Saved. New captures use it now, restart the tray for its folder and shortcut.'

  let config = $state<Config | null>(null)
  let uploaders = $state.raw<string[]>([])
  let hasHotkey = $state(false)
  let saved = $state('')
  let failure = $state('')
  let note = $state('')
  let busy = $state(false)

  const dirty = $derived(config !== null && JSON.stringify(config) !== saved)
  /** A name in the file that has no .sxcu here must not vanish from the list. */
  const uploaderChoices = $derived(
    config?.uploader && !uploaders.includes(config.uploader) ? [config.uploader, ...uploaders] : uploaders,
  )

  async function load() {
    failure = ''
    try {
      const result = await invoke<{ config: Config; uploaders: string[]; has_hotkey: boolean }>('load_preferences')
      config = result.config
      uploaders = result.uploaders
      hasHotkey = result.has_hotkey
      saved = JSON.stringify(config)
    } catch (error) {
      config = null
      failure = String(error)
    }
  }

  async function save() {
    if (!config || busy) return
    busy = true
    note = ''
    failure = ''
    try {
      const sent = $state.snapshot(config)
      await invoke('save_preferences', { config: sent })
      saved = JSON.stringify(config)
      note = SAVED
      await applyTheme()
    } catch (error) {
      failure = String(error)
    } finally {
      busy = false
    }
  }

  async function openFile() {
    try {
      await invoke('open_config')
    } catch (error) {
      failure = String(error)
    }
  }

  function toggle(list: string[], name: string, on: boolean) {
    const index = list.indexOf(name)
    if (on && index < 0) list.push(name)
    if (!on && index >= 0) list.splice(index, 1)
  }

  function emptyToNull(value: string): string | null {
    return value.trim() === '' ? null : value
  }

  function onKeyDown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === 's') {
      event.preventDefault()
      if (dirty) save()
    }
  }

  onMount(load)
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="settings">
  <main>
    {#if config}
      <section>
        <h2>Appearance</h2>
        <label class="row">
          <span class="name">Theme</span>
          <select bind:value={config.ui.theme}>
            <option value="system">Same as the system</option>
            <option value="latte">Catppuccin Latte</option>
            <option value="frappe">Catppuccin Frappé</option>
            <option value="macchiato">Catppuccin Macchiato</option>
            <option value="mocha">Catppuccin Mocha</option>
          </select>
        </label>
      </section>

      <section>
        <h2>After a capture</h2>
        <div class="row">
          <span class="name">Do this</span>
          <div class="choices">
            {#each AFTER_CAPTURE as [name, label] (name)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={config.after_capture.includes(name)}
                  onchange={(event) => toggle(config!.after_capture, name, event.currentTarget.checked)}
                />
                {label}
              </label>
            {/each}
          </div>
        </div>
        <div class="row">
          <span class="name">After an upload</span>
          <div class="choices">
            {#each AFTER_UPLOAD as [name, label] (name)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={config.after_upload.includes(name)}
                  onchange={(event) => toggle(config!.after_upload, name, event.currentTarget.checked)}
                />
                {label}
              </label>
            {/each}
          </div>
        </div>
        <label class="row">
          <span class="name">Uploader</span>
          <select
            value={config.uploader ?? ''}
            onchange={(event) => (config!.uploader = emptyToNull(event.currentTarget.value))}
          >
            <option value="">None</option>
            {#each uploaderChoices as name (name)}
              <option value={name}>{name}</option>
            {/each}
          </select>
        </label>
        <label class="row">
          <span class="name">Notifications</span>
          <input type="checkbox" bind:checked={config.notifications} />
        </label>
      </section>

      <section>
        <h2>Files</h2>
        <label class="row">
          <span class="name">Folder</span>
          <input
            type="text"
            spellcheck="false"
            placeholder="Pictures/accioshot"
            value={config.save_dir ?? ''}
            oninput={(event) => (config!.save_dir = emptyToNull(event.currentTarget.value))}
          />
        </label>
        <label class="row">
          <span class="name">File name</span>
          <span class="stack">
            <input type="text" spellcheck="false" bind:value={config.name_pattern} />
            <small>%y %mo %d %h %mi %s date, %t title, %pn program, %ra&#123;8&#125; random</small>
          </span>
        </label>
      </section>

      <section>
        <h2>Capturing</h2>
        <label class="row">
          <span class="name">Delay in seconds</span>
          <input class="number" type="number" min="0" step="0.5" bind:value={config.delay} />
        </label>
        <label class="row">
          <span class="name">Click selects a window</span>
          <input type="checkbox" bind:checked={config.region.snap_windows} />
        </label>
        <label class="row">
          <span class="name">Magnifier</span>
          <input type="checkbox" bind:checked={config.region.magnifier} />
        </label>
        <label class="row">
          <span class="name">Magnifier size</span>
          <input class="number" type="number" min="1" step="1" disabled={!config.region.magnifier} bind:value={config.region.magnifier_size} />
        </label>
        <label class="row">
          <span class="name">Pixels across</span>
          <input class="number" type="number" min="1" step="1" disabled={!config.region.magnifier} bind:value={config.region.magnifier_pixels} />
        </label>
        {#if hasHotkey}
          <label class="row">
            <span class="name">Region shortcut</span>
            <span class="stack">
              <input type="text" spellcheck="false" bind:value={config.tray.region_hotkey} />
              <small>Such as Ctrl+Option+4. Empty turns it off.</small>
            </span>
          </label>
        {/if}
      </section>

      <section>
        <h2>Editor</h2>
        <label class="row">
          <span class="name">Built-in editor</span>
          <input
            type="checkbox"
            checked={config.editor.command === BUILTIN}
            onchange={(event) => (config!.editor.command = event.currentTarget.checked ? BUILTIN : '')}
          />
        </label>
        {#if config.editor.command !== BUILTIN}
          <label class="row">
            <span class="name">Command</span>
            <span class="stack">
              <input type="text" spellcheck="false" placeholder={'satty --filename {input} --output-filename {output}'} bind:value={config.editor.command} />
              <small>&#123;input&#125; and &#123;output&#125; become the file paths.</small>
            </span>
          </label>
        {/if}
      </section>

      <section>
        <h2>History</h2>
        <label class="row">
          <span class="name">Thumbnail width</span>
          <input class="number" type="number" min="1" step="10" bind:value={config.history.thumbnail_width} />
        </label>
        <label class="row">
          <span class="name">Thumbnail height</span>
          <input class="number" type="number" min="1" step="10" bind:value={config.history.thumbnail_height} />
        </label>
        <label class="row">
          <span class="name">Captures shown</span>
          <input class="number" type="number" min="1" step="50" bind:value={config.history.max_items} />
        </label>
        <label class="row">
          <span class="name">Hide missing files</span>
          <input type="checkbox" bind:checked={config.history.filter_missing_files} />
        </label>
      </section>
    {:else if !failure}
      <p class="loading">Loading settings</p>
    {/if}
  </main>

  {#if failure}
    <p class="failure" role="alert">{failure}</p>
  {/if}

  <footer>
    <span class="said" aria-live="polite">{dirty ? '' : note}</span>
    <button class="quiet" title="Tool colors and editor keys live here too" onclick={openFile}>Open config.toml</button>
    {#if config}
      <button class="primary" disabled={!dirty || busy} onclick={save}>Save</button>
    {:else}
      <button class="primary" onclick={load}>Try again</button>
    {/if}
  </footer>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 28px 28px;
    scrollbar-color: var(--active) transparent;
  }
  section {
    max-width: 620px;
    margin: 0 auto;
  }
  h2 {
    margin: 24px 0 8px;
    font-size: 15px;
    font-weight: 600;
  }
  .row {
    display: grid;
    grid-template-columns: 180px 1fr;
    align-items: center;
    gap: 16px;
    min-height: 36px;
  }
  .row:has(.stack),
  .row:has(.choices) {
    align-items: start;
    padding: 4px 0;
  }
  .row:has(.stack) .name,
  .row:has(.choices) .name {
    line-height: 32px;
  }
  .name {
    color: var(--soft);
  }
  .choices {
    display: grid;
    gap: 2px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
  }
  .stack {
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  small {
    color: var(--faint);
    font-size: 12px;
  }
  input[type='text'],
  input[type='number'],
  select {
    width: 100%;
    height: 32px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 0;
    border-radius: 4px;
    background: var(--bar);
    color: inherit;
    font: inherit;
  }
  input::placeholder {
    color: var(--faint);
  }
  input:disabled {
    color: var(--disabled);
  }
  input.number {
    width: 110px;
    font-variant-numeric: tabular-nums;
  }
  /* GTK draws its own control otherwise, and this text can vanish on it. */
  select {
    width: 240px;
    appearance: none;
    -webkit-appearance: none;
    padding-right: 28px;
    background: var(--bar) var(--caret) no-repeat right 10px center;
  }
  /* GTK's own follow its theme and not this one, and can read backwards. */
  input[type='checkbox'] {
    appearance: none;
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    margin: 0;
    border-radius: 3px;
    background: var(--hover);
    transition: background-color 100ms;
  }
  input[type='checkbox']:hover {
    background-color: var(--active);
  }
  input[type='checkbox']:checked {
    background: var(--text) var(--check);
  }
  input[type='number']::-webkit-inner-spin-button {
    -webkit-appearance: none;
  }
  .loading {
    margin: 48px auto;
    max-width: 620px;
    color: var(--muted);
  }
  .failure {
    margin: 0;
    padding: 10px 28px;
    background: var(--failure-bg);
    color: var(--failure-text);
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px 8px 28px;
    background: var(--bar);
  }
  .said {
    flex: 1;
    color: var(--soft);
  }
</style>
