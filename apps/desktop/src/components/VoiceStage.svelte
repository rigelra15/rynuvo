<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import type { Channel, Id } from '../lib/types'
  import VoiceIcon from './VoiceIcon.svelte'

  let { channel, onMenu }: { channel: Channel; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channel.id))
  const joined = $derived(store.inVoice(channel.id))
</script>

<section class="stage">
  {#if list.length}
    <div class="grid">
      {#each list as m (m.userId)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="tile" class:talk={store.speaking.has(m.userId)} oncontextmenu={(e) => { e.preventDefault(); onMenu(m.userId, e) }}>
          <span class="av">{store.nick(m.userId).slice(0, 1).toUpperCase()}</span>
          <span class="n">
            {store.nick(m.userId)}
            {#if m.deafened}<VoiceIcon name="headphonesOff" size={14} />{:else if m.muted}<VoiceIcon name="micOff" size={14} />{/if}
          </span>
        </div>
      {/each}
    </div>
  {:else}
    <p class="empty muted">Nobody is here yet.</p>
  {/if}

  {#if joined}
    {#if store.voicePath === 'ws'}<p class="path muted">via server (WebSocket)</p>{/if}
    <div class="bar">
      <button class="ctl" class:off={store.myVoice.muted} title={store.myVoice.muted ? 'Unmute' : 'Mute'} onclick={() => store.toggleMute()}>
        <VoiceIcon name={store.myVoice.muted ? 'micOff' : 'mic'} />
      </button>
      <button class="ctl" class:off={store.myVoice.deafened} title={store.myVoice.deafened ? 'Undeafen' : 'Deafen'} onclick={() => store.toggleDeafen()}>
        <VoiceIcon name={store.myVoice.deafened ? 'headphonesOff' : 'headphones'} />
      </button>
      <button class="ctl" disabled title="Screen share is coming soon"><VoiceIcon name="screen" /></button>
      <button class="ctl leave" title="Leave voice" onclick={() => store.leaveVoice()}><VoiceIcon name="leave" /></button>
    </div>
  {:else}
    <div class="bar"><button class="btn primary" onclick={() => store.joinVoice(channel.id)}>Join voice</button></div>
  {/if}
</section>

<style>
  .stage { flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 18px; gap: 14px; }
  .grid { flex: 1; display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; align-content: center; overflow-y: auto; }
  .tile { aspect-ratio: 16 / 10; background: var(--panel); border-radius: 12px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; box-shadow: inset 0 0 0 2px transparent; transition: box-shadow 80ms; }
  .tile.talk { box-shadow: inset 0 0 0 2px var(--speaking); }
  .av { width: 64px; height: 64px; border-radius: 50%; background: var(--accent); color: #fff; display: grid; place-items: center; font-size: 26px; font-weight: 700; }
  .tile.talk .av { box-shadow: 0 0 0 3px var(--speaking); }
  .n { display: flex; align-items: center; gap: 6px; font-weight: 600; color: var(--faint); }
  .n { color: var(--text); }
  .empty { flex: 1; display: grid; place-items: center; }
  .path { text-align: center; font-size: 12px; margin: 0; }
  .bar { display: flex; justify-content: center; gap: 10px; }
  .ctl { width: 46px; height: 46px; border-radius: 50%; background: var(--elev); display: grid; place-items: center; color: var(--text); }
  .ctl:hover:not(:disabled) { background: var(--elev-2); }
  .ctl.off { background: rgba(239, 68, 68, 0.16); }
  .ctl:disabled { opacity: 0.4; cursor: default; }
  .ctl.leave { background: var(--danger); color: #fff; }
</style>
