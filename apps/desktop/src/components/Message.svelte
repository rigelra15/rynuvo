<script lang="ts">
  import { render } from '../lib/markdown'
  import { store } from '../lib/state.svelte'
  import { api, errText } from '../lib/api'
  import type { Message } from '../lib/types'

  let { m, head }: { m: Message; head: boolean } = $props()

  let editing = $state(false)
  let draft = $state('')
  let error = $state<string | null>(null)

  const mine = $derived(store.me?.id === m.authorId)
  const canDelete = $derived(mine || (store.selected ? store.canManage(store.selected) : false))
  const html = $derived(render(m.content))

  function time(ms: number) {
    const d = new Date(ms)
    const today = new Date().toDateString() === d.toDateString()
    const t = d.toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' })
    return today ? t : `${d.toLocaleDateString('id-ID', { day: '2-digit', month: '2-digit' })} ${t}`
  }

  function startEdit() {
    draft = m.content
    editing = true
  }

  async function saveEdit() {
    try {
      await api.editMessage(m.id, draft)
      editing = false
      error = null
    } catch (e) {
      error = errText(e)
    }
  }

  async function remove() {
    if (!confirm('Hapus pesan ini?')) return
    try {
      await api.deleteMessage(m.id)
    } catch (e) {
      error = errText(e)
    }
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      void saveEdit()
    } else if (e.key === 'Escape') editing = false
  }
</script>

<div class="msg" class:head>
  {#if head}
    <div class="av">{store.nick(m.authorId).slice(0, 1).toUpperCase()}</div>
    <div class="meta"><span class="who">{store.nick(m.authorId)}</span><span class="time">{time(m.createdAt)}</span></div>
  {/if}
  <div class="body">
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value={draft} onkeydown={editKey} rows="2" autofocus></textarea>
      <div class="hint">Enter simpan · Esc batal</div>
    {:else}
      {#if m.content}<div class="text selectable">{@html html}</div>{/if}
      {#if m.editedAt}<span class="edited">(diedit)</span>{/if}
    {/if}
    {#if error}<div class="err">{error}</div>{/if}
  </div>
  {#if !editing && (mine || canDelete)}
    <div class="actions">
      {#if mine}<button title="Edit" onclick={startEdit}>✎</button>{/if}
      {#if canDelete}<button title="Hapus" onclick={remove}>🗑</button>{/if}
    </div>
  {/if}
</div>

<style>
  .msg { position: relative; display: grid; grid-template-columns: 40px 1fr; column-gap: 12px; padding: 2px 18px; }
  .msg.head { margin-top: 12px; }
  .msg:hover { background: rgba(255, 255, 255, 0.025); }
  .av { grid-row: span 2; width: 36px; height: 36px; border-radius: 50%; background: var(--elev); display: grid; place-items: center; font-weight: 700; color: var(--accent); }
  .meta { display: flex; gap: 8px; align-items: baseline; }
  .who { font-weight: 600; color: #fff; }
  .time { font-size: 11px; color: var(--faint); }
  .body { grid-column: 2; min-width: 0; }
  .text { word-wrap: break-word; overflow-wrap: anywhere; }
  .text :global(p) { margin: 0; }
  .text :global(code) { font-family: var(--mono); background: var(--panel-2); padding: 1px 4px; border-radius: 4px; font-size: 13px; }
  .text :global(pre) { background: var(--panel-2); padding: 10px; border-radius: 6px; overflow-x: auto; margin: 4px 0; }
  .text :global(pre code) { background: none; padding: 0; }
  .text :global(a) { color: #a99cf9; }
  .edited { font-size: 11px; color: var(--faint); }
  textarea { width: 100%; resize: none; }
  .hint { font-size: 11px; color: var(--faint); }
  .err { color: var(--danger); font-size: 12px; }
  .actions { position: absolute; right: 16px; top: -10px; display: none; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .msg:hover .actions { display: flex; }
  .actions button { padding: 3px 8px; color: var(--muted); }
  .actions button:hover { color: var(--text); background: var(--elev); }
</style>
