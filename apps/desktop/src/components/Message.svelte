<script lang="ts">
  import Icon from './Icon.svelte'
  import Avatar from './Avatar.svelte'
  import { untrack } from 'svelte'
  import { mentionsMe } from '../lib/mention'
  import { avatarColor, nameColor } from '../lib/avatar'
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
  const members = $derived(store.people())
  const html = $derived(render(m.content, { users: members, me: store.me?.id }))
  // Someone else @mentioned me (or @everyone): the bubble gets a gold edge.
  const mentioned = $derived(!mine && !!store.me && mentionsMe(m.content, members, store.me.id))

  // Just sent or received (not history): slides in.
  const fresh = untrack(() => Date.now() - m.createdAt < 4000) // decided once, at mount
  const author = $derived(store.user(m.authorId))
  const whoColor = $derived(nameColor(avatarColor(author ?? { id: m.authorId })))

  // The day is in the date chip above; the bubble shows the time only.
  const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })

  function startEdit() {
    draft = m.content
    editing = true
    picked = false
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
  // Touch screens have no hover: a long press (or right click) shows the message's actions until a
  // tap lands elsewhere.
  let picked = $state(false)
  let rowEl: HTMLDivElement | undefined = $state()
  $effect(() => {
    if (!picked) return
    const away = (e: PointerEvent) => { if (!rowEl?.contains(e.target as Node)) picked = false }
    window.addEventListener('pointerdown', away, true)
    return () => window.removeEventListener('pointerdown', away, true)
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="row" class:mine class:head class:fresh class:picked bind:this={rowEl}
  oncontextmenu={(e) => { if (editing || !(mine || canDelete)) return; e.preventDefault(); picked = !picked }}>
  {#if !mine}
    <div class="side">{#if head}<Avatar user={author} id={m.authorId} size={32} />{/if}</div>
  {/if}
  <div class="bubble" class:mentioned class:editing class:has-img={!!m.image && !editing} class:img-only={!!m.image && !m.content && !editing}>
    {#if head && !mine}<div class="who" style="color:{whoColor}">{store.nick(m.authorId)}</div>{/if}
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
  .row { display: flex; align-items: flex-end; gap: 8px; padding: 1px 22px; }
  .row.head { margin-top: 10px; align-items: flex-start; }
  .row.mine { flex-direction: row-reverse; }
  .side { width: 32px; flex-shrink: 0; }
  .fresh { animation: msg-in 240ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes msg-in { from { opacity: 0; transform: translateY(8px) scale(0.98); } }
  .fresh.mine { transform-origin: bottom right; }
  .fresh:not(.mine) { transform-origin: bottom left; }
  @media (prefers-reduced-motion: reduce) { .fresh { animation: none; } }

  .bubble {
    position: relative; min-width: 0; max-width: 68%; padding: 7px 12px 6px;
    background: var(--elev); color: var(--text); border-radius: 18px;
  }
  .head:not(.mine) .bubble { border-top-left-radius: 6px; }
  .mine .bubble { background: var(--bubble-mine); color: var(--on-accent); }
  .mine.head .bubble { border-top-right-radius: 6px; }
  /* A picture runs to the bubble's edges (the bubble's own corners round it off); its shape is the
     picture's own, never cropped. */
  .bubble.has-img { padding-top: 0; overflow: hidden; }
  .has-img .who { padding-top: 7px; margin-bottom: 5px; }
  /* Just a picture: it is the bubble, with the time over its corner. */
  .bubble.img-only { padding: 0; }
  .img-only .who { padding: 7px 12px 0; }
  .img-only .content { position: absolute; right: 8px; bottom: 8px; }
  .bubble.img-only .stamp { padding: 4px 7px; border-radius: 999px; color: #fff; background: rgba(10, 8, 14, 0.55); backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); }
  .bubble.editing { width: min(70%, 420px); max-width: none; }

  /* Someone called you: the bubble catches the lamp's light. */
  .bubble.mentioned { background: #3a2c22; box-shadow: 0 0 0 1.5px var(--accent); }
  .text :global(.mention) { padding: 0 4px; border-radius: 6px; font-weight: 650; background: rgba(196, 181, 255, 0.16); color: var(--lilac); }
  .text :global(.mention.me) { background: rgba(244, 184, 96, 0.2); color: #ffd79a; }
  .mine .text :global(.mention), .mine .text :global(.mention.me) { background: rgba(42, 26, 5, 0.14); color: var(--on-accent); }
  .who { font-size: 13px; font-weight: 700; margin-bottom: 1px; }

  .content { display: flex; flex-wrap: wrap; align-items: flex-end; column-gap: 10px; }
  .text { flex: 1 1 auto; min-width: 0; overflow-wrap: anywhere; word-break: break-word; line-height: 1.4; }
  .stamp { margin-left: auto; font-size: 10.5px; line-height: 1; padding-bottom: 1px; white-space: nowrap; color: var(--muted); }
  .mine .stamp { color: rgba(42, 26, 5, 0.66); }

  .text :global(p) { margin: 0; }
  .text :global(code) { font-family: var(--mono); background: rgba(0, 0, 0, 0.25); padding: 1px 4px; border-radius: 4px; font-size: 13px; }
  .text :global(pre) { background: rgba(0, 0, 0, 0.28); padding: 8px 10px; border-radius: 8px; overflow-x: auto; margin: 4px 0; max-width: 100%; }
  .text :global(pre code) { background: none; padding: 0; }
  .text :global(a) { color: var(--lilac); }
  .mine .text :global(a) { color: var(--on-accent); text-decoration: underline; }

  .img { display: block; max-width: calc(100% + 24px); margin: 0 -12px 6px; padding: 0; border-radius: 0; overflow: hidden; background: var(--panel-2); cursor: zoom-in; }
  .img-only .img { max-width: 100%; margin: 0; }
  .img img { width: 100%; height: 100%; object-fit: cover; display: block; }

  textarea { width: 100%; resize: none; }
  .hint { font-size: 11px; color: var(--faint); }
  .mine .hint { color: rgba(42, 26, 5, 0.7); }
  .err { color: #fca5a5; font-size: 12px; }
  .mine .err { color: #6b1010; font-weight: 600; }

  .actions { display: flex; align-self: center; visibility: hidden; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }
  .row:hover .actions, .row.picked .actions { visibility: visible; }
  .actions button { padding: 4px 6px; color: var(--muted); display: grid; place-items: center; }
  .actions button:hover { color: var(--text); background: var(--elev); }
</style>
