<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { topBar } from '../lib/update'
  import { updater } from '../lib/updater.svelte'

  const bar = $derived(topBar({
    outdated: store.updateRequired,
    online: store.conn.state === 'online',
    busy: store.myVoice.channelId !== null || store.live !== null,
    update: updater.state,
  }))
</script>

{#if bar && updater.state.phase === 'idle'}
  <div class="bar" class:warn={bar.kind === 'outdated'}>
    {bar.kind === 'outdated' ? 'This version of Rynuvo is outdated. Checking for an update…' : 'Reconnecting…'}
  </div>
{/if}

<style>
  .bar { padding: 6px 12px; font-size: 13px; font-weight: 600; background: #3b3416; color: #fde68a; text-align: center; }
  .warn { background: #3b1414; color: #fca5a5; }
</style>
