<script lang="ts">
  import { tick } from 'svelte'
  import Icon from './Icon.svelte'
  import { api, errText } from '../lib/api'
  import type { Channel } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { mentionQuery, suggest } from '../lib/mention'
  import { fly } from 'svelte/transition'

  let { channel }: { channel: Channel } = $props()
  const MAX = 4000
  let text = $state('')
  let error = $state<string | null>(null)
  let sending = $state(false)
  let lastTyping = 0
  let file = $state<File | null>(null)
  let preview = $state<string | null>(null)
  let dragging = $state(false)
  let area: HTMLTextAreaElement | undefined = $state()

  // @mention autocomplete
  let query = $state<{ start: number; query: string } | null>(null)
  let pick = $state(0)
  const options = $derived(query ? suggest(query.query, [...store.users.values()], store.me?.id) : [])

  function updateQuery() {
    query = area ? mentionQuery(text, area.selectionStart) : null
    pick = 0
  }

  async function choose(nick: string) {
    if (!query || !area) return
    const end = area.selectionStart
    const insert = `@${nick} `
    text = text.slice(0, query.start) + insert + text.slice(end)
    const caret = query.start + insert.length
    query = null
    await tick()
    area.focus()
    area.setSelectionRange(caret, caret)
  }

  function attach(f: File | null | undefined) {
    if (!f || !f.type.startsWith('image/')) return
    if (preview) URL.revokeObjectURL(preview)
    file = f
    preview = URL.createObjectURL(f)
    error = null
  }

  function clearAttach() {
    if (preview) URL.revokeObjectURL(preview)
    file = null
    preview = null
  }

  function onPaste(e: ClipboardEvent) {
    const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith('image/'))
    if (item) {
      e.preventDefault()
      attach(item.getAsFile())
    }
  }

  function onDrop(e: DragEvent) {
    e.preventDefault()
    dragging = false
    attach(e.dataTransfer?.files?.[0])
  }

  const left = $derived(MAX - [...text].length)

  async function send() {
    const content = text.trim()
    if ((!content && !file) || sending || left < 0) return
    sending = true
    const cid = channel.id // the target channel never changes mid-send
    try {
      if (file) {
        const ref = await api.uploadImage(new Uint8Array(await file.arrayBuffer()))
        await api.postImageMessage(cid, content, ref.id)
        clearAttach()
      } else {
        await api.postMessage(cid, content)
      }
      text = ''
      error = null
    } catch (e) {
      error = errText(e)
    } finally {
      sending = false
    }
  }

  function onKey(e: KeyboardEvent) {
    if (options.length && !e.isComposing) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        pick = (pick + (e.key === 'ArrowDown' ? 1 : options.length - 1)) % options.length
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        void choose(options[pick].nickname)
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        query = null
        return
      }
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      void send()
    }
  }

  function onInput() {
    updateQuery()
    const now = Date.now()
    if (now - lastTyping > 3000) {
      lastTyping = now
      void api.sendTyping(channel.id)
    }
  }
</script>

<div class="composer">
  {#if error}<div class="err">{error}</div>{/if}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="box"
    class:drag={dragging}
    ondragover={(e) => { e.preventDefault(); dragging = true }}
    ondragleave={() => (dragging = false)}
    ondrop={onDrop}
  >
    {#if preview}
      <div class="att">
        <img src={preview} alt="" />
        {#if sending}<span class="busy">Compressing…</span>{:else}<button class="x" title="Cancel" onclick={clearAttach}><Icon name="close" size={14} /></button>{/if}
      </div>
    {/if}
    {#if options.length}
      <div class="mentions" role="listbox" aria-label="Mention someone" transition:fly={{ y: 6, duration: 140 }}>
        <div class="mh">Members</div>
        {#each options as o, i (o.id)}
          <button
            role="option"
            aria-selected={i === pick}
            class:on={i === pick}
            onmouseenter={() => (pick = i)}
            onmousedown={(e) => { e.preventDefault(); void choose(o.nickname) }}
          >
            {#if o.id === -1}<span class="all">@</span>{:else}<Avatar user={store.user(o.id)} id={o.id} size={22} />{/if}
            <span>{o.id === -1 ? '@everyone' : o.nickname}</span>
            {#if o.id === -1}<span class="hint">Notify everyone</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={area}
      onclick={updateQuery}
      onkeyup={(e) => (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') && updateQuery()}
      onblur={() => (query = null)}
      onpaste={onPaste}
      bind:value={text}
      onkeydown={onKey}
      oninput={onInput}
      rows="1"
      placeholder="Message #{channel.name}"
    ></textarea>
    {#if left < 200}<span class="count" class:over={left < 0}>{left}</span>{/if}
  </div>
</div>

<style>
  .composer { padding: 0 18px 18px; }
  .box { position: relative; background: var(--elev-2); border-radius: 10px; }
  textarea { width: 100%; resize: none; background: transparent; border: 0; padding: 12px 14px; max-height: 200px; field-sizing: content; }
  .box.drag { outline: 2px dashed var(--accent); }
  .att { position: relative; display: inline-block; margin: 10px 12px 0; }
  .att img { max-height: 120px; max-width: 240px; border-radius: 6px; display: block; }
  .x { position: absolute; top: -8px; right: -8px; width: 22px; height: 22px; border-radius: 50%; background: var(--danger); color: #fff; font-size: 11px; display: grid; place-items: center; }
  .busy { position: absolute; inset: 0; display: grid; place-items: center; background: rgba(0,0,0,.55); color: #fff; font-size: 12px; border-radius: 6px; }
  .count { position: absolute; right: 10px; bottom: 8px; font-size: 11px; color: var(--muted); }
  .count.over { color: var(--danger); }
  .mentions { position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); z-index: 5; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 6px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35); }
  .mh { font-size: 11px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); padding: 4px 8px 6px; }
  .mentions button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 6px 8px; border-radius: 6px; text-align: left; color: var(--text); }
  .mentions button.on { background: var(--elev); }
  .all { width: 22px; height: 22px; border-radius: 50%; background: var(--elev); display: grid; place-items: center; font-weight: 700; color: var(--muted); }
  .hint { margin-left: auto; font-size: 12px; color: var(--muted); }
  .err { color: var(--danger); font-size: 12px; padding: 0 2px 6px; }
</style>
