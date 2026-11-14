<script lang="ts">
  import { api, errText } from '../lib/api'
  import type { Channel } from '../lib/types'

  let { channel }: { channel: Channel } = $props()
  const MAX = 4000
  let text = $state('')
  let error = $state<string | null>(null)
  let sending = $state(false)
  let lastTyping = 0
  let file = $state<File | null>(null)
  let preview = $state<string | null>(null)
  let dragging = $state(false)

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
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      void send()
    }
  }

  function onInput() {
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
        {#if sending}<span class="busy">Compressing…</span>{:else}<button class="x" title="Cancel" onclick={clearAttach}>✕</button>{/if}
      </div>
    {/if}
    <textarea
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
  .x { position: absolute; top: -8px; right: -8px; width: 22px; height: 22px; border-radius: 50%; background: var(--danger); color: #fff; font-size: 11px; }
  .busy { position: absolute; inset: 0; display: grid; place-items: center; background: rgba(0,0,0,.55); color: #fff; font-size: 12px; border-radius: 6px; }
  .count { position: absolute; right: 10px; bottom: 8px; font-size: 11px; color: var(--muted); }
  .count.over { color: var(--danger); }
  .err { color: var(--danger); font-size: 12px; padding: 0 2px 6px; }
</style>
