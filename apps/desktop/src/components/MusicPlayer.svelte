<script lang="ts">
  import Slider from './Slider.svelte'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import LyricsView from './LyricsView.svelte'
  import { store } from '../lib/state.svelte'
  import { frameClock, lyricsOf } from '../lib/lyrics.svelte'
  import { clock, LYRICS_SOURCES, mayControl, positionMs, type MusicState } from '../lib/music'
  import { membersOf } from '../lib/voice'

  let { music }: { music: { state: MusicState; receivedAt: number } } = $props()
  const s = $derived(music.state)
  const now = $derived(s.now!)
  const paused = $derived(s.pausedAt !== null)

  // Every frame while playing, so the bar and the lyrics glide instead of stepping.
  const frame = frameClock(() => !paused)
  const pos = $derived(positionMs(s, music.receivedAt, frame.now))
  const pct = $derived(now.durationMs ? (pos / now.durationMs) * 100 : 0)

  const inVoice = $derived(membersOf(store.voice, s.channelId).map((m) => m.userId))
  const me = $derived({ id: store.me?.id ?? 0, admin: !!store.me?.isAdmin })
  const canControl = $derived(mayControl(now.requestedBy, me, inVoice))
  const onlyWho = $derived(`Only ${store.nick(now.requestedBy)} can control this song`)
  const status = $derived(store.musicStatus.get(now.videoId))
  const repeat = $derived(s.repeat ?? 'off')
  const source = $derived(s.lyrics ?? 'auto')
  const lyrics = $derived(lyricsOf(now, source))
  const sourceName = (id: string) => LYRICS_SOURCES.find((x) => x.id === id)?.name ?? id
  // Only the channel's owner or an admin picks where lyrics come from; everyone in the call follows.
  const channel = $derived(store.channels.find((c) => c.id === s.channelId))
  const canPick = $derived(!!channel && store.canManage(channel))
  let picking = $state(false)
  function pickSource(id: string) {
    picking = false
    if (id !== source) void store.musicCmd({ op: 'lyrics', source: id })
  }

</script>

<aside class="player" class:paused>
  <div class="head">
    <div class="sleeve">
      <div class="disc"><div class="label" style="background-image:url('{now.artUrl}')"></div></div>
      <div class="cover" style="background-image:url('{now.artUrl}')"></div>
    </div>
    <div class="info">
      <div class="kicker">{paused ? 'Paused' : 'Now playing'}{repeat === 'song' ? ' · on loop' : ''}</div>
      <div class="title" title={now.title}>{now.title}</div>
      <div class="artist">{now.artist}{now.album ? ` · ${now.album}` : ''}</div>
      <span class="who" title="Requested by {store.nick(now.requestedBy)}"><Avatar user={store.user(now.requestedBy)} id={now.requestedBy} size={16} />{store.nick(now.requestedBy)}</span>
    </div>
  </div>
  {#if status?.status === 'error'}
    <div class="warn">Couldn't play this song here: {status.message}</div>
  {:else if status?.status === 'loading'}
    <div class="note">Loading the song…</div>
  {/if}
  <div class="seek">
    <div class="bar" role="progressbar" aria-label="Song position" aria-valuemin={0} aria-valuemax={now.durationMs} aria-valuenow={Math.round(pos)}>
      <div class="fill" style="transform:scaleX({pct / 100})"></div>
    </div>
    <div class="times"><span>{clock(pos)}</span><span>{clock(now.durationMs)}</span></div>
  </div>
  <div class="ctl">
    <button class="big" disabled={!canControl} title={canControl ? (paused ? 'Resume' : 'Pause') : onlyWho} onclick={() => store.musicCmd({ op: paused ? 'resume' : 'pause' })}>
      <Icon name={paused ? 'play' : 'pause'} />
    </button>
    <button disabled={!canControl} title={canControl ? 'Skip' : onlyWho} onclick={() => store.musicCmd({ op: 'skip' })}><Icon name="skip" /></button>
    <button
      class:on={repeat !== 'off'}
      title={repeat === 'off' ? 'Loop: off (click for queue)' : repeat === 'queue' ? 'Loop: queue (click for song)' : 'Loop: this song (click to turn off)'}
      aria-label="Loop: {repeat}"
      onclick={() => store.musicCmd({ op: 'repeat', mode: null })}
    ><Icon name={repeat === 'song' ? 'repeatOne' : 'repeat'} /></button>
    <button class:on={store.queueOpen} title="Queue" onclick={() => (store.queueOpen = !store.queueOpen)}><Icon name="queue" /></button>
    <label class="vol" title="Music volume (only for you)">
      <Icon name="volume" size={16} />
      <Slider value={store.musicVolume} step={0.05} tone="light" label="Music volume" oninput={(v) => store.setMusicVolume(v)} />
    </label>
  </div>
  {#if s.queue.length}
    <div class="next">Up next: {s.queue[0].artist} — {s.queue[0].title}{s.queue.length > 1 ? ` · ${s.queue.length - 1} more` : ''}</div>
  {/if}

  {#if !store.queueOpen}
    <div class="src">
      <button
        class="chip"
        disabled={!canPick}
        aria-haspopup="menu"
        aria-expanded={picking}
        title={canPick ? 'Change where lyrics come from (for everyone here)' : "Only the channel's owner or an admin can change the lyrics source"}
        onclick={() => (picking = !picking)}
      >
        Lyrics · {sourceName(source)}{source === 'auto' && lyrics?.source ? ` (${sourceName(lyrics.source)})` : ''}
        {#if canPick}<span class="caret" aria-hidden="true">▾</span>{/if}
      </button>
      {#if picking}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="scrim" onmousedown={() => (picking = false)}></div>
        <div class="menu" role="menu">
          {#each LYRICS_SOURCES as o (o.id)}
            <button role="menuitemradio" aria-checked={o.id === source} class:on={o.id === source} onclick={() => pickSource(o.id)}>
              <span>{o.name}</span>{#if o.id === source}<Icon name="check" size={14} />{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
  <div class="below">
    {#if store.queueOpen}
      <div class="queue">
        <h3>Queue</h3>
        {#if !s.queue.length}<p class="empty">The queue is empty</p>{/if}
        <ol>
          {#each s.queue as e, i (e.id)}
            <li>
              <span class="qart" style="background-image:url('{e.artUrl}')"></span>
              <span class="qtxt"><b>{e.title}</b><small>{e.artist} · {clock(e.durationMs)}</small></span>
              <span class="who" title="Requested by {store.nick(e.requestedBy)}"><Avatar user={store.user(e.requestedBy)} id={e.requestedBy} size={16} />{store.nick(e.requestedBy)}</span>
              {#if mayControl(e.requestedBy, me, inVoice)}
                <button title="Remove from the queue" onclick={() => store.musicCmd({ op: 'remove', index: i + 1 })}><Icon name="close" size={14} /></button>
              {/if}
            </li>
          {/each}
        </ol>
      </div>
    {:else if lyrics?.lines.length}
      <LyricsView lines={lyrics.lines} {pos} />
    {:else if lyrics?.plain}
      <div class="plain"><pre>{lyrics.plain}</pre></div>
    {:else}
      <p class="none">{lyrics === undefined ? 'Looking for lyrics…' : 'No lyrics for this song'}</p>
    {/if}
  </div>
</aside>

<style>
  .player {
    flex: none; width: clamp(320px, 34%, 460px); min-height: 0; display: flex; flex-direction: column; gap: 12px; color: #fff;
    padding: 18px; border-radius: 16px; background: rgba(10, 10, 16, 0.32); backdrop-filter: blur(18px);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .head { display: flex; gap: 16px; align-items: center; min-width: 0; }
  .sleeve { position: relative; flex: none; width: 112px; height: 112px; margin-right: 58px; }
  .cover, .label { background-size: cover; background-position: center; background-color: #2a2b35; }
  .cover { position: absolute; inset: 0; border-radius: 10px; z-index: 2; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5); }
  .disc {
    position: absolute; z-index: 1; width: 108px; height: 108px; left: 62px; top: 2px; border-radius: 50%; display: grid; place-items: center;
    background: repeating-radial-gradient(circle, #111 0 2px, #1c1c22 2px 4px); box-shadow: 0 8px 22px rgba(0, 0, 0, 0.5);
    animation: spin 6s linear infinite;
  }
  .paused .disc { animation-play-state: paused; }
  /* The album art on the record's label, with a spindle hole. */
  .label { position: relative; width: 46px; height: 46px; border-radius: 50%; box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.6); }
  .label::after { content: ''; position: absolute; left: 50%; top: 50%; width: 6px; height: 6px; margin: -3px; border-radius: 50%; background: #111; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .info { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .kicker { text-transform: uppercase; letter-spacing: 0.08em; font-size: 10px; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 800; font-size: 22px; line-height: 1.15; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .artist { color: rgba(255, 255, 255, 0.78); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .warn { font-size: 12px; color: #ffb4b4; }
  .note { font-size: 12px; color: rgba(255, 255, 255, 0.7); }
  .seek { display: flex; flex-direction: column; gap: 2px; }
  .times { display: flex; justify-content: space-between; font-size: 11px; color: rgba(255, 255, 255, 0.7); font-variant-numeric: tabular-nums; }
  .bar { position: relative; height: 4px; margin: 6px 0 4px; border-radius: 2px; overflow: hidden; background: rgba(255, 255, 255, 0.2); }
  .fill { position: absolute; inset: 0; background: #fff; transform-origin: left; }
  .ctl { display: flex; gap: 10px; align-items: center; }
  .ctl button { width: 38px; height: 38px; border-radius: 50%; background: rgba(255, 255, 255, 0.14); display: grid; place-items: center; color: #fff; transition: background 120ms, transform 120ms; }
  .ctl button:hover:not(:disabled) { background: rgba(255, 255, 255, 0.24); }
  .ctl button:active:not(:disabled) { transform: scale(0.94); }
  .ctl button.big { width: 44px; height: 44px; background: #fff; color: #16171c; }
  .ctl button.on { background: rgba(255, 255, 255, 0.34); }
  .ctl button:disabled { opacity: 0.4; cursor: not-allowed; }
  .vol { flex: 1; display: flex; align-items: center; gap: 6px; margin-left: 4px; color: rgba(255, 255, 255, 0.8); min-width: 0; }
  .vol :global(.slider) { flex: 1; min-width: 40px; max-width: 120px; }
  .next { font-size: 12px; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .src { position: relative; display: flex; }
  .chip {
    display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 999px; font-size: 11.5px; font-weight: 600;
    color: rgba(255, 255, 255, 0.85); background: rgba(255, 255, 255, 0.1); transition: background 120ms;
  }
  .chip:hover:not(:disabled) { background: rgba(255, 255, 255, 0.18); }
  .chip:disabled { cursor: default; color: rgba(255, 255, 255, 0.6); }
  .caret { font-size: 9px; opacity: 0.7; }
  .scrim { position: fixed; inset: 0; z-index: 4; }
  .menu {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 5; min-width: 180px; padding: 4px; border-radius: 10px;
    background: rgba(20, 20, 28, 0.92); backdrop-filter: blur(18px); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45), inset 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
  .menu button { width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 7px 10px; border-radius: 7px; font-size: 13px; color: #fff; text-align: left; }
  .menu button:hover { background: rgba(255, 255, 255, 0.1); }
  .menu button.on { color: #fff; font-weight: 600; }
  .below { flex: 1; min-height: 0; border-top: 1px solid rgba(255, 255, 255, 0.08); padding-top: 6px; }
  .plain { height: 100%; overflow: hidden; mask-image: linear-gradient(transparent, #000 15%, #000 85%, transparent); }
  .plain pre { margin: 0; font: inherit; font-size: 15px; font-weight: 600; white-space: pre-wrap; color: rgba(255, 255, 255, 0.75); animation: crawl 240s linear infinite; }
  @keyframes crawl { to { transform: translateY(-70%); } }
  .none { height: 100%; margin: 0; display: grid; place-items: center; color: rgba(255, 255, 255, 0.5); font-size: 14px; }
  .queue { height: 100%; overflow-y: auto; }
  .queue h3 { margin: 4px 0 8px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.7); }
  .queue ol { list-style: none; margin: 0; padding: 0; }
  .queue li { display: flex; align-items: center; gap: 10px; padding: 6px; border-radius: 8px; }
  .queue li:hover { background: rgba(255, 255, 255, 0.08); }
  .qart { width: 36px; height: 36px; border-radius: 6px; background-size: cover; background-color: #2a2b35; flex: none; }
  .qtxt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .qtxt b, .qtxt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .qtxt small { color: rgba(255, 255, 255, 0.65); }
  .who {
    flex: none; align-self: flex-start; display: inline-flex; align-items: center; gap: 5px; max-width: 120px; padding: 2px 8px 2px 2px; border-radius: 999px;
    font-size: 11px; font-weight: 600; color: rgba(255, 255, 255, 0.9); background: rgba(255, 255, 255, 0.12); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .info .who { margin-top: 4px; }
  .queue li .who { align-self: center; }
  .queue button { color: rgba(255, 255, 255, 0.7); display: grid; padding: 4px; border-radius: 6px; }
  .queue button:hover { color: #fff; background: rgba(255, 255, 255, 0.12); }
  .empty { color: rgba(255, 255, 255, 0.6); font-size: 13px; }
  @media (prefers-reduced-motion: reduce) {
    .disc, .plain pre { animation: none; }
  }
</style>
