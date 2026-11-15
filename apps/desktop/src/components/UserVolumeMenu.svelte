<script lang="ts">
  import Slider from './Slider.svelte'
  import { menu } from '../lib/motion'
  import { api, errText } from '../lib/api'
  import { dialog } from '../lib/dialog.svelte'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'

  let { userId, x, y, onClose }: { userId: Id; x: number; y: number; onClose: () => void } = $props()

  const member = $derived(store.voice.find((m) => m.userId === userId))
  const channel = $derived(store.channels.find((c) => c.id === member?.channelId))
  const isMe = $derived(userId === store.me?.id)
  const canKick = $derived(!isMe && !!channel && store.canManage(channel))
  let pct = $state(Math.round((store.volumes.get(userId) ?? 1) * 100))

  function change() {
    store.setVolume(userId, pct / 100)
  }

  async function kick() {
    // Read who before closing: closing the menu drops its props, `userId` included.
    const who = userId
    onClose()
    try {
      await api.voiceKick(who)
    } catch (e) {
      void dialog.notice(errText(e))
    }
  }
</script>

<svelte:window onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} />
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="menu" transition:menu|global style="left:{x}px; top:{y}px" onclick={(e) => e.stopPropagation()}>
  <div class="who">{store.nick(userId)}</div>
  {#if !isMe}
    <label>
      <span>User volume <b>{pct}%</b></span>
      <Slider bind:value={pct} max={200} step={1} label="User volume" oninput={change} />
    </label>
  {/if}
  {#if canKick}<button class="danger" onclick={kick}>Disconnect</button>{/if}
  {#if isMe}<p class="muted">That's you.</p>{/if}
</div>

<style>
  .menu { position: fixed; z-index: 60; background: var(--panel-2); border: 1px solid var(--line); border-radius: 14px; padding: 8px; width: 220px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); display: flex; flex-direction: column; gap: 8px; }
  .who { font-weight: 600; padding: 0 2px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  button { text-align: left; padding: 7px 10px; border-radius: 9px; }
  button:hover { background: var(--elev); }
  .danger { color: #f87171; }
  p { margin: 0; font-size: 12px; }
</style>
