<script lang="ts">
  import { store } from '../lib/state.svelte'
</script>

{#if store.updateRequired}
  <div class="bar warn">This version of Rynuvo is outdated. Please update.</div>
{:else if store.conn.state !== 'online'}
  <div class="bar">Reconnecting…</div>
{/if}

<style>
  .bar { padding: 6px 12px; text-align: center; font-size: 13px; font-weight: 600; background: #3b3416; color: #fde68a; }
  .warn { background: #3b1414; color: #fca5a5; }
</style>
