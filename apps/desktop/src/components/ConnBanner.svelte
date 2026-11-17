<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { topBar } from '../lib/update'
  import { updater } from '../lib/updater.svelte'

  let online = $state(navigator.onLine)
  $effect(() => {
    const set = () => (online = navigator.onLine)
    window.addEventListener('online', set)
    window.addEventListener('offline', set)
    return () => {
      window.removeEventListener('online', set)
      window.removeEventListener('offline', set)
    }
  })
  const bar = $derived(topBar({
    outdated: store.updateRequired,
    online: store.conn.state === 'online',
    busy: store.myVoice.channelId !== null || store.live !== null,
    update: updater.state,
  }))
</script>

{#if bar && updater.state.phase === 'idle'}
  <div class="bar" class:warn={bar.kind === 'outdated'}>
    {bar.kind === 'outdated' ? 'This version of Rynuvo is outdated. Checking for an update…' : online ? 'Reconnecting…' : 'No internet connection · Reconnecting…'}
  </div>
{/if}

<style>
  /* Above the grey that covers the app while offline: the one thing still in colour. */
  .bar { position: relative; z-index: 91; padding: 6px 12px; font-size: 13px; font-weight: 600; background: #3b3416; color: #fde68a; text-align: center; }
  .warn { background: #3b1414; color: #fca5a5; }
</style>
