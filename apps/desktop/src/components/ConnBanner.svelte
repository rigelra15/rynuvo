<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { banner } from '../lib/update'
  import { updater } from '../lib/updater.svelte'

  const update = $derived(banner(updater.state, store.myVoice.channelId !== null || store.live !== null))
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

{#if store.updateRequired}
  <div class="bar warn">This version of Rynuvo is outdated. Please update.</div>
{:else if store.conn.state !== 'online'}
  <div class="bar">Reconnecting…</div>
{:else if update}
  <div class="bar ok">
    {update.text}
    {#if update.canRestart}
      <button onclick={restart} disabled={restarting}>{restarting ? 'Restarting…' : 'Restart'}</button>
    {:else}
      <span class="hint">{update.hint}</span>
    {/if}
  </div>
{/if}

<style>
  .bar { padding: 6px 12px; text-align: center; font-size: 13px; font-weight: 600; background: #3b3416; color: #fde68a; }
  .warn { background: #3b1414; color: #fca5a5; }
  .ok { background: #14301f; color: #86efac; display: flex; gap: 10px; justify-content: center; align-items: center; }
  .ok button { padding: 2px 10px; border-radius: 4px; border: 0; background: #22c55e; color: #052e16; font-weight: 700; cursor: pointer; }
  .ok button:disabled { opacity: 0.6; cursor: default; }
  .hint { font-weight: 400; opacity: 0.8; }
</style>
