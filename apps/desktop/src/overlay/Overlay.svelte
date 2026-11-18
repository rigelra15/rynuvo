<script lang="ts">
  // The call overlay's own page (its window floats over other apps). It only draws what the main
  // window sends and hands its buttons back.
  import { onMount, tick } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { emitTo, listen } from '@tauri-apps/api/event'
  import Avatar from '../components/Avatar.svelte'
  import Icon from '../components/Icon.svelte'
  import { ACTION_EVENT, READY_EVENT, STATE_EVENT, type OverlayAction, type OverlayState } from '../lib/overlay'

  let s = $state<OverlayState | null>(null)
  /** Taking clicks (the shortcut was pressed); otherwise they pass through to what's beneath. */
  let live = $state(false)
  let card: HTMLDivElement | undefined = $state()
  const isMac = navigator.userAgent.includes('Mac')
  const MAX = 8

  // Talkers first, so the ones you want are always in view.
  const shown = $derived(s ? [...s.people].sort((a, b) => Number(b.talking) - Number(a.talking)).slice(0, MAX) : [])
  const extra = $derived(s ? Math.max(0, s.people.length - MAX) : 0)

  const act = (a: OverlayAction) => void emitTo('main', ACTION_EVENT, a)

  // The window is as tall as what's in it.
  $effect(() => {
    void [s, live]
    void tick().then(() => {
      if (card) void invoke('overlay_fit', { height: Math.ceil(card.getBoundingClientRect().height) + 16 })
    })
  })

  onMount(() => {
    const offs = [
      listen<OverlayState>(STATE_EVENT, ({ payload }) => (s = payload)),
      listen<boolean>('rynuvo://overlay-mode', ({ payload }) => (live = payload)),
    ]
    void emitTo('main', READY_EVENT, null)
    return () => offs.forEach((p) => void p.then((f) => f()))
  })
</script>

{#if s}
  <div class="card" class:live bind:this={card}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="head" onmousedown={(e) => { if (live && e.button === 0 && !(e.target as HTMLElement).closest('button')) void invoke('overlay_drag') }}>
      <Icon name="volume" size={13} />
      <span class="room">{s.room}</span>
      {#if live}
        <button class="lock" title="Let clicks through" onclick={() => void invoke('overlay_lock')}><Icon name="lock" size={13} /></button>
      {/if}
    </div>
    <ul>
      {#each shown as p (p.id)}
        <li class:talking={p.talking}>
          <Avatar user={p.user} id={p.id} size={26} speaking={p.talking} />
          <span class="name">{p.me ? 'You' : p.name}</span>
          {#if p.deafened}<span class="st"><Icon name="headphonesOff" size={13} /></span>
          {:else if p.muted}<span class="st"><Icon name="micOff" size={13} /></span>{/if}
        </li>
      {/each}
      {#if extra}<li class="more">+{extra} more</li>{/if}
    </ul>
    {#if live}
      <div class="ctl">
        <button class:off={s.muted} onclick={() => act('mute')} title={s.muted ? 'Unmute' : 'Mute'}><Icon name={s.muted ? 'micOff' : 'mic'} size={16} /></button>
        <button class:off={s.deafened} onclick={() => act('deafen')} title={s.deafened ? 'Undeafen' : 'Deafen'}><Icon name={s.deafened ? 'headphonesOff' : 'headphones'} size={16} /></button>
        <button onclick={() => void invoke('overlay_open')} title="Open Rynuvo"><Icon name="expand" size={16} /></button>
        <button class="leave" onclick={() => act('leave')} title="Leave"><Icon name="leave" size={16} /></button>
      </div>
      <p class="tip">{isMac ? '⌘⇧O' : 'Ctrl+Shift+O'} or the lock lets clicks through again</p>
    {:else}
      <p class="tip quiet">{isMac ? '⌘⇧O' : 'Ctrl+Shift+O'} to use</p>
    {/if}
  </div>
{/if}

<style>
  :global(html), :global(body), :global(#app) { background: transparent !important; overflow: hidden; }
  .card {
    margin: 8px; padding: 10px 10px 8px; border-radius: 16px;
    background: rgba(24, 19, 31, 0.78); border: 1px solid rgba(255, 255, 255, 0.07);
    backdrop-filter: blur(14px); -webkit-backdrop-filter: blur(14px);
    color: var(--text); font-size: 13px; user-select: none; -webkit-user-select: none; cursor: default;
    transition: border-color 160ms, background 160ms;
  }
  .card.live { border-color: color-mix(in srgb, var(--accent) 70%, transparent); background: rgba(24, 19, 31, 0.94); }
  .head { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; font-weight: 600; padding: 0 2px 6px; }
  .live .head { cursor: grab; }
  .room { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lock { color: var(--muted); display: grid; place-items: center; width: 22px; height: 22px; border-radius: 6px; }
  .lock:hover { color: var(--text); background: rgba(255, 255, 255, 0.08); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  li { display: flex; align-items: center; gap: 9px; opacity: 0.62; transition: opacity 160ms; padding: 1px 2px; }
  li.talking { opacity: 1; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6); }
  .st { color: var(--danger); display: grid; }
  .more { color: var(--muted); font-size: 12px; padding-left: 36px; }
  .ctl { display: flex; gap: 6px; margin-top: 10px; }
  .ctl button { flex: 1; height: 32px; border-radius: 10px; display: grid; place-items: center; color: var(--text); background: rgba(255, 255, 255, 0.07); }
  .ctl button:hover { background: rgba(255, 255, 255, 0.13); }
  .ctl button.off { color: var(--danger); background: color-mix(in srgb, var(--danger) 18%, transparent); }
  .ctl .leave { color: #fff; background: var(--danger); }
  .ctl .leave:hover { background: color-mix(in srgb, var(--danger) 85%, white); }
  .tip { margin: 7px 2px 0; font-size: 11px; color: var(--muted); }
  .tip.quiet { opacity: 0.55; }
</style>
