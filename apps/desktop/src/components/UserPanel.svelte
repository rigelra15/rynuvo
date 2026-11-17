<script lang="ts">
  import { isMobile } from '../lib/platform'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import CallTimer from './CallTimer.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel, User } from '../lib/types'

  let { me, onSettings, onOpenCall }: { me: User; onSettings: () => void; onOpenCall?: (c: Channel) => void } = $props()
  const voiceChannel = $derived(store.channels.find((c) => c.id === store.myVoice.channelId) ?? null)
</script>

{#if voiceChannel}
  <div class="vc">
    <!-- The call I'm in: tap to go back to its room. -->
    <button class="vct" title="Open {voiceChannel.name}" onclick={() => onOpenCall?.(voiceChannel)}>
      <span class="ok">Voice connected</span>
      <span class="muted">{voiceChannel.name}<CallTimer channelId={voiceChannel.id} prefix=" · " />{store.voicePath === 'ws' ? ' · via server' : ''}</span>
    </button>
    <button class="gear" title="Leave voice" onclick={() => store.leaveVoice()}><Icon name="leave" /></button>
  </div>
{/if}
<footer class="me">
  <span class="av"><Avatar user={me} id={me.id} size={30} speaking={store.speaking.has(me.id)} /><span class="dot" class:mobile={isMobile}></span></span>
  <span class="who">
    <span class="nick">{me.nickname}</span>
    <span class="status">{store.conn.state === 'online' ? 'Online' : 'Connecting…'}</span>
  </span>
  <button class="gear" class:off={store.myVoice.muted} title={store.myVoice.muted ? 'Unmute' : 'Mute'} onclick={() => store.toggleMute()}>
    <Icon name={store.myVoice.muted ? 'micOff' : 'mic'} size={16} />
  </button>
  <button class="gear" class:off={store.myVoice.deafened} title={store.myVoice.deafened ? 'Undeafen' : 'Deafen'} onclick={() => store.toggleDeafen()}>
    <Icon name={store.myVoice.deafened ? 'headphonesOff' : 'headphones'} size={16} />
  </button>
  <button class="gear" title="Settings" onclick={onSettings}><Icon name="settings" size={16} /></button>
</footer>

<style>
  .me { display: flex; align-items: center; gap: 8px; margin: 0 10px 10px; padding: 8px 8px 8px 10px; background: var(--panel); border: 1px solid var(--line); border-radius: 14px; }
  .av { position: relative; display: inline-flex; }
  .dot.mobile { width: 12px; height: 17px; border-radius: 4px; }
  .dot.mobile::after { content: ''; position: absolute; left: 1px; right: 1px; top: 1px; bottom: 3px; border-radius: 1px; background: var(--panel); }
  .dot { position: absolute; right: -2px; bottom: -2px; z-index: 2; width: 12px; height: 12px; border-radius: 50%; background: var(--speaking); border: 3px solid var(--panel); box-sizing: border-box; }
  .who { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.2; }
  .nick { font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { font-size: 11.5px; color: var(--muted); }
  .gear { color: var(--muted); padding: 6px; border-radius: 9px; display: grid; place-items: center; }
  .gear:hover { color: var(--text); background: var(--elev); }
  .gear.off { color: var(--danger); }
  .vc { display: flex; align-items: center; gap: 4px; margin: 0 10px 6px; padding: 4px 8px 4px 4px; border-radius: 14px; background: var(--accent-soft); border: 1px solid rgba(244, 184, 96, 0.25); }
  .vct {
    flex: 1; display: flex; flex-direction: column; align-items: flex-start; font-size: 12px; min-width: 0; text-align: left;
    padding: 4px 8px; border-radius: 10px; color: inherit; transition: background 160ms, transform 160ms;
  }
  .vct:hover { background: rgba(244, 184, 96, 0.1); }
  .vct:active { transform: scale(0.98); background: rgba(244, 184, 96, 0.14); }
  .vct .muted { max-width: 100%; }
  .vct .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ok { color: var(--accent); font-weight: 700; }
</style>
