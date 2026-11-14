<script lang="ts">
  import Icon from './Icon.svelte'
  import Avatar from './Avatar.svelte'
  import { avatarColor } from '../lib/avatar'
  import { render } from '../lib/markdown'
  import { store } from '../lib/state.svelte'
  import { api, errText, mediaUrl } from '../lib/api'
  import ImageViewer from './ImageViewer.svelte'
  import { dialog } from '../lib/dialog.svelte'
  import type { Message } from '../lib/types'

  let { m, head }: { m: Message; head: boolean } = $props()

  let editing = $state(false)
  let draft = $state('')
  let error = $state<string | null>(null)
  let viewing = $state(false)

  /** Same 400×300 fit as before; the bubble can still shrink it (max-width: 100%). */
  function fit(w: number, h: number) {
    const k = Math.min(1, 400 / w, 300 / h)
    return `width:${Math.round(w * k)}px;aspect-ratio:${w}/${h}`
  }

  const mine = $derived(store.me?.id === m.authorId)
  const canDelete = $derived(mine || (store.selected ? store.canManage(store.selected) : false))
  const html = $derived(render(m.content))

  const author = $derived(store.user(m.authorId))
  const nameColor = $derived(avatarColor(author ?? { id: m.authorId }))

  // The day is in the date chip above; the bubble shows the time only.
  const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })

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
    if (!(await dialog.ask('Delete this message?', 'Delete'))) return
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

<div class="row" class:mine class:head>
  {#if !mine}
    <div class="side">{#if head}<Avatar user={author} id={m.authorId} size={32} />{/if}</div>
  {/if}
  <div class="bubble" class:editing class:has-img={!!m.image}>
    {#if head && !mine}<div class="who" style="color:{nameColor}">{store.nick(m.authorId)}</div>{/if}
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value={draft} onkeydown={editKey} rows="2" autofocus></textarea>
      <div class="hint">Enter to save · Esc to cancel</div>
    {:else}
      {#if m.image}
        <button class="img" style={fit(m.image.width, m.image.height)} onclick={() => (viewing = true)}>
          <img src={mediaUrl(m.image.id)} alt="" loading="lazy" />
        </button>
      {/if}
      <div class="content">
        {#if m.content}<div class="text selectable">{@html html}</div>{/if}
        <span class="stamp">{#if m.editedAt}edited · {/if}{time(m.createdAt)}</span>
      </div>
    {/if}
    {#if error}<div class="err">{error}</div>{/if}
  </div>
  {#if !editing && (mine || canDelete)}
    <div class="actions">
      {#if mine}<button title="Edit" onclick={startEdit}><Icon name="edit" size={15} /></button>{/if}
      {#if canDelete}<button title="Delete" onclick={remove}><Icon name="trash" size={15} /></button>{/if}
    </div>
  {/if}
</div>
{#if viewing && m.image}<ImageViewer image={m.image} onclose={() => (viewing = false)} />{/if}

<style>
  .row { display: flex; align-items: flex-end; gap: 8px; padding: 1px 18px; }
  .row.head { margin-top: 10px; align-items: flex-start; }
  .row.mine { flex-direction: row-reverse; }
  .side { width: 32px; flex-shrink: 0; }

  .bubble {
    position: relative; min-width: 0; max-width: 70%; padding: 6px 10px 5px;
    background: var(--elev); color: var(--text); border-radius: 14px;
    box-shadow: 0 1px 1px rgba(0, 0, 0, 0.18);
  }
  .head:not(.mine) .bubble { border-top-left-radius: 4px; }
  .mine .bubble { background: var(--bubble-mine); color: #fff; }
  .mine.head .bubble { border-top-right-radius: 4px; }
  .bubble.has-img { padding-top: 4px; }
  .bubble.editing { width: min(70%, 420px); max-width: none; }

  .who { font-size: 13px; font-weight: 600; margin-bottom: 1px; }

  .content { display: flex; flex-wrap: wrap; align-items: flex-end; column-gap: 10px; }
  .text { flex: 1 1 auto; min-width: 0; overflow-wrap: anywhere; word-break: break-word; line-height: 1.4; }
  .stamp { margin-left: auto; font-size: 10.5px; line-height: 1; padding-bottom: 1px; white-space: nowrap; color: var(--muted); }
  .mine .stamp { color: rgba(255, 255, 255, 0.7); }

  .text :global(p) { margin: 0; }
  .text :global(code) { font-family: var(--mono); background: rgba(0, 0, 0, 0.25); padding: 1px 4px; border-radius: 4px; font-size: 13px; }
  .text :global(pre) { background: rgba(0, 0, 0, 0.28); padding: 8px 10px; border-radius: 8px; overflow-x: auto; margin: 4px 0; max-width: 100%; }
  .text :global(pre code) { background: none; padding: 0; }
  .text :global(a) { color: #b4a9ff; }
  .mine .text :global(a) { color: #fff; text-decoration: underline; }

  .img { display: block; max-width: 100%; margin: 0 -6px 4px; padding: 0; border-radius: 10px; overflow: hidden; background: var(--panel-2); cursor: zoom-in; }
  .img img { width: 100%; height: 100%; object-fit: cover; display: block; }

  textarea { width: 100%; resize: none; }
  .hint { font-size: 11px; color: var(--faint); }
  .mine .hint { color: rgba(255, 255, 255, 0.7); }
  .err { color: #fca5a5; font-size: 12px; }

  .actions { display: flex; align-self: center; visibility: hidden; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .row:hover .actions { visibility: visible; }
  .actions button { padding: 4px 6px; color: var(--muted); display: grid; place-items: center; }
  .actions button:hover { color: var(--text); background: var(--elev); }
</style>
