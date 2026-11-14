<script lang="ts">
  import { dialog } from '../lib/dialog.svelte'

  function key(e: KeyboardEvent) {
    if (!dialog.current || (e.key !== 'Escape' && e.key !== 'Enter')) return
    e.preventDefault()
    dialog.answer(e.key === 'Enter')
  }
</script>

<svelte:window onkeydown={key} />

{#if dialog.current}
  {@const d = dialog.current}
  <div class="overlay">
    <div class="box" role="alertdialog" aria-modal="true">
      <p>{d.message}</p>
      <div class="actions">
        {#if d.cancel}<button class="btn" onclick={() => dialog.answer(false)}>{d.cancel}</button>{/if}
        <button class="btn" class:danger={d.danger} class:primary={!d.danger} onclick={() => dialog.answer(true)}>{d.ok}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay { position: fixed; inset: 0; background: rgba(8, 8, 12, 0.6); display: grid; place-items: center; z-index: 100; }
  .box { width: min(400px, 90vw); background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 20px; }
  p { margin: 0 0 18px; line-height: 1.45; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
