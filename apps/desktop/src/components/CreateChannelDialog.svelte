<script lang="ts">
  import { backdrop, pop } from '../lib/motion'
  import { errText } from '../lib/api'

  let {
    title,
    initial = '',
    confirm = 'Save',
    onSubmit,
    onClose,
  }: { title: string; initial?: string; confirm?: string; onSubmit: (name: string) => Promise<void>; onClose: () => void } = $props()

  // svelte-ignore state_referenced_locally
  let name = $state(initial)
  let error = $state<string | null>(null)
  let busy = $state(false)

  async function submit(e: SubmitEvent) {
    e.preventDefault()
    busy = true
    try {
      await onSubmit(name)
      onClose()
    } catch (err) {
      error = errText(err)
    } finally {
      busy = false
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />
<div class="overlay" transition:backdrop|global role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <form class="dlg" transition:pop|global onsubmit={submit}>
    <h2>{title}</h2>
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={name} maxlength="40" autofocus required />
    {#if error}<p class="err">{error}</p>{/if}
    <div class="row">
      <button type="button" class="btn" onclick={onClose}>Cancel</button>
      <button class="btn primary" disabled={busy}>{confirm}</button>
    </div>
  </form>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 50; }
  .dlg { width: 380px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  h2 { margin: 0; font-size: 17px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  .err { color: var(--danger); margin: 0; font-size: 13px; }
</style>
