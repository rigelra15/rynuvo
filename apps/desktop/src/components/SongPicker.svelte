<script lang="ts">
  // The chat composer's song picker: SongSearch in a popover.
  import SongSearch from './SongSearch.svelte'
  import { fly } from 'svelte/transition'
  import type { Id } from '../lib/types'

  let { textChannelId, onClose }: { textChannelId: Id; onClose: () => void } = $props()
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="scrim" onmousedown={onClose}></div>
<div class="picker" role="dialog" aria-label="Play a song" transition:fly={{ y: 8, duration: 160 }}>
  <SongSearch {textChannelId} onPlayed={onClose} onEscape={onClose} />
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 4; }
  .picker {
    position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); z-index: 5; max-width: 520px; margin-left: auto;
    background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 8px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
  }
</style>
