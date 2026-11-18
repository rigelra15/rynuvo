<script lang="ts">
  // The call overlay's own page (its window floats over other apps). It only draws what the main
  // window sends and hands its buttons back: who's in the call, and chat messages as they come in.
  import { onMount, tick } from 'svelte'
  import { fly } from 'svelte/transition'
  import { invoke } from '@tauri-apps/api/core'
  import { emitTo, listen } from '@tauri-apps/api/event'
  import Avatar from '../components/Avatar.svelte'
  import Icon from '../components/Icon.svelte'
  import { mediaUrl } from '../lib/api'
  import { ACTION_EVENT, CHAT_EVENT, READY_EVENT, STATE_EVENT, type OverlayAction, type OverlayChat, type OverlayState } from '../lib/overlay'

  let s = $state<OverlayState | null>(null)
  /** Taking clicks (the shortcut was pressed); otherwise they pass through to what's beneath. */
  let live = $state(false)
  let chats = $state<OverlayChat[]>([])
  let box: HTMLDivElement | undefined = $state()
  const MAX = 10
  const CHATS = 3

  // In a steady order (by name), so nobody jumps around as people talk.
  const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' })
  const shown = $derived(s ? [...s.people].sort((a, b) => byName(a.name, b.name)).slice(0, MAX) : [])
  const extra = $derived(s ? Math.max(0, s.people.length - MAX) : 0)

  const act = (a: OverlayAction) => void emitTo('main', ACTION_EVENT, a)

  /** A message stays a while (longer with a picture), then goes. */
  function arrive(c: OverlayChat) {
    chats = [...chats, c].slice(-CHATS)
    setTimeout(() => (chats = chats.filter((x) => x.id !== c.id)), c.image ? 9000 : 6500)
  }

  const drag = (e: MouseEvent) => {
    if (live && e.button === 0 && !(e.target as HTMLElement).closest('button')) void invoke('overlay_drag')
  }

  // The window is as tall as what's in it.
  $effect(() => {
    void [s, live, chats]
    void tick().then(() => {
      if (box) void invoke('overlay_fit', { height: Math.ceil(box.getBoundingClientRect().height) + 4 })
    })
  })

  onMount(() => {
    const offs = [
      listen<OverlayState>(STATE_EVENT, ({ payload }) => (s = payload)),
      listen<boolean>('rynuvo://overlay-mode', ({ payload }) => (live = payload)),
      listen<OverlayChat>(CHAT_EVENT, ({ payload }) => arrive(payload)),
    ]
    // Only once all are listening, or the reply (the room as it is) arrives before anyone hears it.
    void Promise.all(offs).then(() => emitTo('main', READY_EVENT, null))
    return () => offs.forEach((p) => void p.then((f) => f()))
  })
</script>

{#if s}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="box" class:live bind:this={box} onmousedown={drag}>
    <ul>
      {#each shown as p (p.id)}
        <li class:talking={p.talking}>
          <Avatar user={p.user} id={p.id} size={26} speaking={p.talking} />
          <span class="pill">
            <span class="name">{p.name}</span>
            {#if p.muted || p.deafened}<Icon name="micOff" size={12} />{/if}
            {#if p.deafened}<Icon name="headphonesOff" size={12} />{/if}
          </span>
        </li>
      {/each}
      {#if extra}<li class="more"><span class="pill">+{extra} more</span></li>{/if}
    </ul>

    {#if live}
      <div class="ctl">
        <button class:off={s.muted} onclick={() => act('mute')} title={s.muted ? 'Unmute' : 'Mute'}><Icon name={s.muted ? 'micOff' : 'mic'} size={16} /></button>
        <button class:off={s.deafened} onclick={() => act('deafen')} title={s.deafened ? 'Undeafen' : 'Deafen'}><Icon name={s.deafened ? 'headphonesOff' : 'headphones'} size={16} /></button>
        <button onclick={() => void invoke('overlay_open')} title="Open Rynuvo"><Icon name="expand" size={16} /></button>
        <button class="leave" onclick={() => act('leave')} title="Leave"><Icon name="leave" size={16} /></button>
        <button onclick={() => void invoke('overlay_lock')} title="Let clicks through"><Icon name="lock" size={15} /></button>
      </div>
    {/if}

    {#if chats.length}
      <div class="chats">
        {#each chats as c (c.id)}
          <div class="chat" in:fly={{ x: -16, duration: 200 }} out:fly={{ x: -16, duration: 220 }}>
            <Avatar user={c.user} id={c.authorId} size={18} />
            <div class="body">
              <p class="from"><b>{c.name}</b><span>#{c.channel}</span></p>
              {#if c.text}<p class="text">{c.text}</p>{/if}
              {#if c.image}<img src={mediaUrl(c.image.id)} alt="" style="aspect-ratio: {c.image.width} / {c.image.height}" />{/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  :global(html), :global(body), :global(#app) { background: transparent !important; overflow: hidden; }
  /* Room for the talking ripple (it grows past the face) without being cut at the window's edge. */
  .box { padding: 12px; color: #fff; font-size: 13px; user-select: none; -webkit-user-select: none; cursor: default; }
  .live { cursor: grab; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 5px; justify-items: start; }
  li { display: flex; align-items: center; gap: 6px; opacity: 0.72; transition: opacity 160ms; }
  li.talking { opacity: 1; }
  /* As wide as the name, on a dim backing so it reads over any game. */
  .pill {
    display: inline-flex; align-items: center; gap: 4px; max-width: 200px; padding: 3px 9px; border-radius: 999px;
    background: rgba(0, 0, 0, 0.3); font-weight: 600; line-height: 1.3;
  }
  .pill :global(svg) { color: #ff6b6b; flex: none; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more .pill { font-weight: 500; font-size: 12px; color: rgba(255, 255, 255, 0.75); margin-left: 32px; }
  .ctl { display: flex; gap: 5px; margin-top: 8px; }
  .ctl button {
    width: 34px; height: 30px; border-radius: 9px; display: grid; place-items: center; color: #fff;
    background: rgba(0, 0, 0, 0.45);
  }
  .ctl button:hover { background: rgba(0, 0, 0, 0.75); }
  .ctl button.off { color: #ff6b6b; }
  .ctl .leave { background: var(--danger); }
  .ctl .leave:hover { background: color-mix(in srgb, var(--danger) 85%, white); }
  .chats { display: grid; gap: 4px; margin-top: 8px; justify-items: start; }
  /* As wide as the message. */
  .chat { display: flex; gap: 6px; align-items: flex-start; max-width: 100%; padding: 5px 9px 5px 6px; border-radius: 11px; background: rgba(0, 0, 0, 0.36); opacity: 0.92; }
  .body { min-width: 0; }
  .from { margin: 0; display: flex; gap: 5px; align-items: baseline; font-size: 11px; line-height: 1.35; }
  .from b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .from span { color: rgba(255, 255, 255, 0.55); font-size: 10px; white-space: nowrap; }
  .text { margin: 1px 0 0; font-size: 11.5px; line-height: 1.3; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  img { display: block; margin-top: 5px; max-width: 100%; max-height: 110px; border-radius: 7px; object-fit: cover; }
</style>
