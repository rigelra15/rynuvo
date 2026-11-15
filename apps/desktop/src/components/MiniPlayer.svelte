<script lang="ts">
  // The music, folded into the room's header while the big panel is hidden: cover, song, play/pause
  // and skip, with a thin line for how far along it is. Clicking the song opens the panel again.
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'
  import { frameClock } from '../lib/lyrics.svelte'
  import { mayControl, positionMs, type MusicState } from '../lib/music'
  import { membersOf } from '../lib/voice'
  import { videos } from '../lib/videos'
  import { follow } from '../lib/video'

  let { music, onOpen }: { music: { state: MusicState; receivedAt: number }; onOpen: () => void } = $props()
  const s = $derived(music.state)
  const now = $derived(s.now!)
  const paused = $derived(s.pausedAt !== null)
  const frame = frameClock(() => !paused)
  const pos = $derived(positionMs(s, music.receivedAt, frame.now))
  const pct = $derived(now.durationMs ? Math.min(1, pos / now.durationMs) : 0)
  const inVoice = $derived(membersOf(store.voice, s.channelId).map((m) => m.userId))
  const canControl = $derived(mayControl(now.requestedBy, { id: store.me?.id ?? 0, admin: !!store.me?.isAdmin }, inVoice))

  // A music video plays here too, silent, in step with the room (the sound comes from the room's player).
  let videoUrl = $state<string | null>(null)
  let video = $state<HTMLVideoElement | null>(null)
  let ready = $state(false)
  $effect(() => {
    const id = now.video ? now.videoId : null
    videoUrl = null
    ready = false
    if (!id) return
    let live = true
    videos.get(id).then((u) => { if (live) videoUrl = u }, () => {})
    return () => { live = false }
  })
  $effect(() => {
    const v = video
    if (v && ready) follow(v, pos, paused)
  })
</script>

<div class="mini" class:paused>
  <button class="song" title="Show the music panel" onclick={onOpen}>
    <span class="art" class:wide={now.video} style="background-image:url('{now.artUrl}')">
      {#if videoUrl}<video class:on={ready} bind:this={video} src={videoUrl} muted playsinline preload="auto"
        onloadeddata={() => (ready = true)} onerror={() => { videoUrl = null; ready = false }}></video>{/if}
    </span>
    <span class="txt"><b>{now.title}</b><small>{now.artist}</small></span>
  </button>
  <button class="b" disabled={!canControl} title={paused ? 'Resume' : 'Pause'} onclick={() => store.musicCmd({ op: paused ? 'resume' : 'pause' })}>
    <Icon name={paused ? 'play' : 'pause'} size={16} />
  </button>
  <button class="b" title="Skip" onclick={() => store.musicCmd({ op: 'skip' })}><Icon name="skip" size={16} /></button>
  <span class="line" style="transform:scaleX({pct})" aria-hidden="true"></span>
</div>

<style>
  .mini {
    position: relative; overflow: hidden; display: flex; align-items: center; gap: 2px; height: 36px; padding: 3px 4px 3px 3px; border-radius: 12px;
    background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line); max-width: 340px;
  }
  .song { display: flex; align-items: center; gap: 9px; min-width: 0; flex: 1; padding: 1px 8px 1px 1px; border-radius: 9px; text-align: left; }
  .song:hover { background: var(--elev-2); }
  .art { flex: none; position: relative; overflow: hidden; width: 28px; height: 28px; border-radius: 7px; background-size: cover; background-position: center; background-color: var(--elev); }
  .art.wide { width: 50px; border-radius: 6px; }
  .art video { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: 0; transition: opacity 300ms ease-out; }
  .art video.on { opacity: 1; }
  .txt { min-width: 0; display: flex; flex-direction: column; line-height: 1.15; }
  .txt b, .txt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt b { font-size: 12.5px; font-weight: 650; }
  .txt small { font-size: 11.5px; color: var(--muted); }
  .b { flex: none; width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; color: var(--text); transition: background 160ms; }
  .b:hover:not(:disabled) { background: var(--elev); }
  .b:disabled { opacity: 0.4; cursor: not-allowed; }
  .line { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: var(--accent); transform-origin: left; }
  .paused .line { opacity: 0.5; }
</style>
