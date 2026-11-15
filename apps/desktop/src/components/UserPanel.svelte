<script lang="ts">
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import CallTimer from './CallTimer.svelte'
  import { store } from '../lib/state.svelte'
  import type { User } from '../lib/types'

  let { me, onSettings }: { me: User; onSettings: () => void } = $props()
  const voiceChannel = $derived(store.channels.find((c) => c.id === store.myVoice.channelId) ?? null)
</script>

{#if voiceChannel}
  <div class="vc">
    <div class="vct">
      <span class="ok">Voice connected</span>
      <span class="muted">{voiceChannel.name}<CallTimer channelId={voiceChannel.id} prefix=" · " />{store.voicePath === 'ws' ? ' · via server' : ''}</span>
    </div>
    <button class="gear" title="Leave voice" onclick={() => store.leaveVoice()}><Icon name="leave" /></button>
  </div>
{/if}
<footer class="me">
  <span class="av"><Avatar user={me} id={me.id} size={30} speaking={store.speaking.has(me.id)} /><span class="dot"></span></span>
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
  .me { display: flex; align-items: center; gap: 8px; padding: 10px 10px; background: var(--panel-2); }
  .av { position: relative; display: inline-flex; }
  .dot { position: absolute; right: -2px; bottom: -2px; z-index: 2; width: 12px; height: 12px; border-radius: 50%; background: var(--speaking); border: 3px solid var(--panel-2); box-sizing: border-box; }
  .who { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.2; }
  .nick { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { font-size: 11.5px; color: var(--muted); }
  .gear { color: var(--muted); padding: 4px; border-radius: 6px; display: grid; place-items: center; }
  .gear:hover { color: var(--text); background: var(--elev); }
  .gear.off { color: var(--danger); }
  .vc { display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: var(--panel-2); border-bottom: 1px solid var(--line); }
  .vct { flex: 1; display: flex; flex-direction: column; font-size: 12px; min-width: 0; }
  .vct .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ok { color: var(--speaking); font-weight: 600; }
</style>
