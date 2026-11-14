<script lang="ts">
  import { api, errText } from '../lib/api'
  import type { Channel } from '../lib/types'

  let { channel }: { channel: Channel } = $props()
  const MAX = 4000
  let text = $state('')
  let error = $state<string | null>(null)
  let sending = $state(false)
  let lastTyping = 0

  const left = $derived(MAX - [...text].length)

  async function send() {
    const content = text.trim()
    if (!content || sending || left < 0) return
    sending = true
    try {
      await api.postMessage(channel.id, content)
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
  <div class="box">
    <textarea
      bind:value={text}
      onkeydown={onKey}
      oninput={onInput}
      rows="1"
      placeholder="Kirim pesan ke #{channel.name}"
    ></textarea>
    {#if left < 200}<span class="count" class:over={left < 0}>{left}</span>{/if}
  </div>
</div>

<style>
  .composer { padding: 0 18px 18px; }
  .box { position: relative; background: var(--elev-2); border-radius: 10px; }
  textarea { width: 100%; resize: none; background: transparent; border: 0; padding: 12px 14px; max-height: 200px; field-sizing: content; }
  .count { position: absolute; right: 10px; bottom: 8px; font-size: 11px; color: var(--muted); }
  .count.over { color: var(--danger); }
  .err { color: var(--danger); font-size: 12px; padding: 0 2px 6px; }
</style>
