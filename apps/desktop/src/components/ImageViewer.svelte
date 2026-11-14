<script lang="ts">
  import { api, errText, mediaUrl } from '../lib/api'
  import type { ImageRef } from '../lib/types'

  let { image, onclose }: { image: ImageRef; onclose: () => void } = $props()
  let note = $state<string | null>(null)

  async function save() {
    try {
      note = `Saved: ${await api.saveImage(image.id)}`
    } catch (e) {
      note = errText(e)
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onclick={onclose}>
  <img src={mediaUrl(image.id)} alt="" onclick={(e) => e.stopPropagation()} />
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="bar" onclick={(e) => e.stopPropagation()}>
    <span>{image.width}×{image.height}</span>
    <button class="btn" onclick={save}>Save</button>
    <button class="btn ghost" onclick={onclose}>Close</button>
    {#if note}<span class="note">{note}</span>{/if}
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(8, 8, 12, 0.88); display: grid; place-items: center; z-index: 50; }
  img { max-width: 92vw; max-height: 84vh; object-fit: contain; border-radius: 6px; }
  .bar { position: fixed; bottom: 18px; left: 50%; transform: translateX(-50%); display: flex; gap: 10px; align-items: center; background: var(--panel); padding: 8px 12px; border-radius: 10px; color: var(--muted); font-size: 12px; }
  .note { max-width: 360px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
