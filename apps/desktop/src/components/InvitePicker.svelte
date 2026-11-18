<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import { store } from '../lib/state.svelte'
  import { pane } from '../lib/pane.svelte'
  import { phonePortal } from '../lib/portal'
  import { sheet } from '../lib/motion'
  import { membersOf } from '../lib/voice'
  import type { Id } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'

  // "Come hang out": pick who to ask to this room. Online friends first; the ones away get it too,
  // as a notification on their phone. Above the call dock, or a sheet from the bottom on a phone.
  let { channelId, onClose }: { channelId: Id; onClose: () => void } = $props()

  /** The server holds back a repeat to the same person for this long. */
  const AGAIN_MS = 30_000
  let now = $state(Date.now())
  let box = $state<HTMLElement>()
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    // A click anywhere else closes it (the button that opened it toggles it itself).
    const away = (e: PointerEvent) => {
      const t = e.target as Element | null
      if (box && !box.contains(t) && !t?.closest('[data-invite-toggle]')) onClose()
    }
    window.addEventListener('keydown', key)
    window.addEventListener('pointerdown', away, true)
    // Android's Back closes the sheet first (MainActivity asks the page before going back).
    const w = window as unknown as { __rynuvoBack?: () => boolean }
    const back = () => (onClose(), true)
    w.__rynuvoBack = back
    return () => {
      if (w.__rynuvoBack === back) delete w.__rynuvoBack
      clearInterval(t)
      window.removeEventListener('keydown', key)
      window.removeEventListener('pointerdown', away, true)
    }
  })

  const here = $derived(new Set(membersOf(store.voice, channelId).map((m) => m.userId)))
  const people = $derived(
    store
      .people()
      .filter((u) => u.id !== store.me?.id && !here.has(u.id))
      .sort((a, b) => Number(store.online.has(b.id)) - Number(store.online.has(a.id)) || a.nickname.localeCompare(b.nickname)),
  )
  const sent = (id: Id) => now - (store.invited.get(id) ?? 0) < AGAIN_MS
  const waiting = $derived(people.filter((u) => !sent(u.id)))
  const motion = (node: Element) => (pane.narrow ? sheet(node, {}, { direction: 'both' }) : fly(node, { y: 8, duration: 200 }))
</script>

{#if pane.narrow}<div class="scrim" role="presentation" use:phonePortal transition:fade={{ duration: 200 }}></div>{/if}
<div class="pick" bind:this={box} class:phone={pane.narrow} role="dialog" aria-label="Invite to this room" use:phonePortal transition:motion>
  {#if pane.narrow}<span class="grab" aria-hidden="true"></span>{/if}
  <div class="head">
    <b>Invite to this room</b>
    {#if waiting.length > 1}<button class="all" onclick={() => store.invite(waiting.map((u) => u.id))}>Invite everyone</button>{/if}
  </div>
  {#if people.length}
    <ul>
      {#each people as u (u.id)}
        {@const on = store.online.has(u.id)}
        <li>
          <span class="av" class:off={!on}><Avatar user={u} id={u.id} size={34} /></span>
          <span class="who">
            <span class="nm">{u.nickname}</span>
            <span class="st">{on ? (store.voice.some((m) => m.userId === u.id) ? 'In another room' : 'Online') : 'Away, gets a notification'}</span>
          </span>
          {#if sent(u.id)}
            <span class="done" in:fly={{ y: 4, duration: 220 }}><Icon name="check" size={14} /> Invited</span>
          {:else}
            <button class="ask" onclick={() => store.invite([u.id])}>Invite</button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty">Everyone is already in here.</p>
  {/if}
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgba(8, 6, 12, 0.55); }
  .pick {
    position: absolute; z-index: 61; bottom: calc(100% + 12px); left: 50%; translate: -50% 0; width: min(340px, calc(100vw - 32px));
    max-height: min(420px, 60vh); display: flex; flex-direction: column; padding: 14px 14px 10px; border-radius: 18px;
    background: var(--elev); border: 1px solid var(--line); box-shadow: 0 18px 50px rgba(0, 0, 0, 0.5); color: var(--text);
  }
  .pick.phone {
    position: fixed; left: 0; right: 0; bottom: 0; translate: none; width: auto; max-height: 75vh; border-radius: 22px 22px 0 0;
    padding: 8px 18px calc(env(safe-area-inset-bottom) + 18px);
  }
  .grab { align-self: center; width: 40px; height: 4px; border-radius: 2px; background: var(--line); margin-bottom: 12px; }
  .head { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 8px; }
  .head b { font-size: 15px; }
  .all { font-size: 13px; font-weight: 650; color: var(--accent); padding: 6px 8px; border-radius: 9px; }
  .all:hover { background: var(--accent-soft); }
  ul { list-style: none; margin: 0; padding: 0; overflow-y: auto; }
  li { display: flex; align-items: center; gap: 11px; padding: 7px 2px; }
  .av { display: inline-flex; transition: opacity 200ms; }
  .av.off { opacity: 0.5; }
  .who { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .nm { font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st { font-size: 12px; color: var(--muted); }
  .ask { flex: none; padding: 7px 14px; border-radius: 999px; font-weight: 700; font-size: 13px; color: var(--on-accent); background: var(--accent); }
  .ask:active { transform: scale(0.96); }
  .done { flex: none; display: inline-flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 650; color: var(--speaking); padding: 7px 4px; }
  .empty { margin: 8px 0 6px; color: var(--muted); font-size: 14px; }
  @media (pointer: coarse) { li { padding: 9px 2px; } .ask { padding: 9px 16px; } }
</style>
