<script lang="ts">
  import { updater } from '../lib/updater.svelte'

  const state = $derived(updater.state)
  const show = $derived(state.phase === 'ready' || state.phase === 'installing' || state.phase === 'failed')
</script>

{#if show}
  <div class="update-overlay">
    <section class="update-box" role="alertdialog" aria-modal="true"
      aria-labelledby="update-heading" aria-describedby="update-description">
      <div class="mark">↻</div>
      <h2 id="update-heading">Rynuvo update {state.version}</h2>
      {#if state.phase === 'ready'}
        <p id="update-description">A new version has been downloaded. Rynuvo will automatically leave voice, stop screen sharing, and restart to apply the update.</p>
        <div class="counter" aria-live="polite">Restarting in {state.seconds}s</div>
      {:else if state.phase === 'installing'}
        <p id="update-description">Disconnecting voice and screen share, installing the update, and restarting Rynuvo…</p>
        <div class="counter" aria-live="polite">Installing update…</div>
      {:else if state.phase === 'failed'}
        <p id="update-description">The automatic update could not finish. Rynuvo needs to retry before you can continue.</p>
        <p class="error">{state.error}</p>
        <button class="retry" onclick={() => void updater.restart()}>Retry update</button>
      {/if}
    </section>
  </div>
{/if}

<style>
  .update-overlay {
    position: fixed; inset: 0; z-index: 10000;
    background: rgba(8,8,12,0.88); display: grid; place-items: center;
    padding: 20px; backdrop-filter: blur(8px);
  }
  .update-box {
    width: min(430px,100%); border-radius: 20px; padding: 32px;
    border: 1px solid var(--line); background: var(--bg);
    box-shadow: 0 24px 90px rgba(0,0,0,0.55);
    text-align: center; color: var(--text);
  }
  .mark { font-size: 30px; color: #f4b860; margin-bottom: 12px; }
  h2 { margin: 0 0 12px; font-size: 22px; }
  p { line-height: 1.5; font-size: 14px; }
  .counter { margin-top: 20px; font-weight: 700; color: #f4b860; }
  .error { color: #fca5a5; overflow-wrap: anywhere; }
  .retry { border: 0; padding: 10px 20px; border-radius: 8px; background: #f4b860; color: #16171c; font-weight: 700; cursor: pointer; }
</style>
