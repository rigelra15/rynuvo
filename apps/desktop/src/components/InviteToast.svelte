<script lang="ts">
  import { fly } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { store } from '../lib/state.svelte'
  import type { Channel } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'

  // Someone asked me to their room: a card at the top with Join and Not now. It goes by itself after
  // a while; the newest ask is on top.
  let { onJoin }: { onJoin: (c: Channel) => void } = $props()

  const LIFE_MS = 45_000
  const shown = $derived(store.invites.filter((i) => store.channels.some((c) => c.id === i.channelId)).slice(-2).reverse())

  $effect(() => {
    if (!store.invites.length) return
    const oldest = Math.min(...store.invites.map((i) => i.at))
    const t = setTimeout(() => (store.invites = store.invites.filter((i) => Date.now() - i.at < LIFE_MS)), Math.max(0, oldest + LIFE_MS - Date.now()))
    return () => clearTimeout(t)
  })

  function join(i: { channelId: number; from: number }) {
    const c = store.channels.find((c) => c.id === i.channelId)
    store.dismissInvite(i.from)
    if (c) onJoin(c)
  }
</script>

<div class="stack" aria-live="polite">
  {#each shown as i (i.from)}
    {@const room = store.channels.find((c) => c.id === i.channelId)}
    <div class="card" role="alertdialog" aria-label="{store.nick(i.from)} invites you to {room?.name}" in:fly={{ y: -24, duration: 420, easing: cubicOut }} out:fly={{ y: -16, duration: 220 }}>
      <span class="av"><Avatar user={store.user(i.from)} id={i.from} size={42} /><span class="ring" aria-hidden="true"></span></span>
      <span class="txt">
        <b>{store.nick(i.from)}</b> wants you in
        <span class="room"><Icon name="volume" size={13} /> {room?.name}</span>
      </span>
      <span class="btns">
        <button class="later" onclick={() => store.dismissInvite(i.from)}>Not now</button>
        <button class="join" onclick={() => join(i)}>Join</button>
      </span>
      <span class="life" style="animation-duration:{LIFE_MS}ms;animation-delay:{i.at - Date.now()}ms" aria-hidden="true"></span>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed; z-index: 80; top: calc(env(safe-area-inset-top) + 12px); left: 50%; translate: -50% 0; width: min(440px, calc(100vw - 24px));
    display: flex; flex-direction: column; gap: 8px; pointer-events: none;
  }
  .card {
    pointer-events: auto; position: relative; overflow: hidden; display: grid; grid-template-columns: auto 1fr; align-items: center; gap: 4px 12px;
    padding: 12px 14px 12px 12px; border-radius: 18px; color: var(--text);
    background: color-mix(in srgb, var(--elev) 92%, transparent); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid rgba(244, 184, 96, 0.28); box-shadow: 0 16px 44px rgba(0, 0, 0, 0.5), 0 0 24px rgba(244, 184, 96, 0.12);
  }
  .av { position: relative; display: inline-flex; grid-row: span 2; }
  .ring { position: absolute; inset: -4px; border-radius: 50%; border: 2px solid var(--accent); animation: ring 1.8s ease-out infinite; }
  @keyframes ring { from { opacity: 0.9; transform: scale(0.92); } to { opacity: 0; transform: scale(1.3); } }
  .txt { font-size: 14px; line-height: 1.35; color: var(--muted); min-width: 0; }
  .txt b { color: var(--text); }
  .room { display: inline-flex; align-items: center; gap: 4px; color: var(--accent); font-weight: 650; }
  .btns { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  .btns button { padding: 7px 16px; border-radius: 999px; font-weight: 700; font-size: 13.5px; }
  .later { color: var(--muted); background: var(--elev-2); }
  .join { color: var(--on-accent); background: var(--accent); }
  .btns button:active { transform: scale(0.96); }
  /* How long it stays. */
  .life { position: absolute; left: 0; bottom: 0; height: 2px; width: 100%; background: var(--accent); opacity: 0.6; transform-origin: left; animation: life linear forwards; }
  @keyframes life { to { transform: scaleX(0); } }
  @media (prefers-reduced-motion: reduce) { .ring, .life { animation: none; } }
</style>
