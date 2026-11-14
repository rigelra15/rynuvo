<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { topBar } from '../lib/update'
  import { updater } from '../lib/updater.svelte'

  const bar = $derived(
    topBar({
      outdated: store.updateRequired,
      online: store.conn.state === 'online',
      busy: store.myVoice.channelId !== null || store.live !== null,
      update: updater.state,
    }),
  )
  const version = $derived(updater.state.phase === 'ready' ? updater.state.version : '')
  let restarting = $state(false)

  async function restart() {
    restarting = true
    try {
      await updater.restart()
    } finally {
      restarting = false
    }
  }
</script>

{#if bar}
  <div class="bar" class:warn={bar.kind === 'outdated'} class:ok={bar.kind === 'update'}>
    {#if bar.kind === 'outdated'}
      This version of Rynuvo is outdated.{bar.canRestart ? ` Version ${version} is ready.` : ' Please update.'}
    {:else if bar.kind === 'reconnecting'}
      Reconnecting…
    {:else}
      Update ready ({version})
    {/if}
    {#if bar.canRestart}
      <button onclick={restart} disabled={restarting}>{restarting ? 'Restarting…' : 'Restart to update'}</button>
    {:else if bar.kind === 'update'}
      <span class="hint">Leave voice to restart</span>
    {/if}
  </div>
{/if}

<style>
  .bar { padding: 6px 12px; font-size: 13px; font-weight: 600; background: #3b3416; color: #fde68a; display: flex; gap: 10px; justify-content: center; align-items: center; }
  .warn { background: #3b1414; color: #fca5a5; }
  .ok { background: #14301f; color: #86efac; }
  button { padding: 2px 10px; border-radius: 4px; border: 0; background: #22c55e; color: #052e16; font-weight: 700; cursor: pointer; }
  button:disabled { opacity: 0.6; cursor: default; }
  .hint { font-weight: 400; opacity: 0.8; }
</style>
