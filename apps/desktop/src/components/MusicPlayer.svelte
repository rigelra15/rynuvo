<script lang="ts">
  import Slider from './Slider.svelte'
  import Avatar from './Avatar.svelte'
  import SongSearch from './SongSearch.svelte'
  import Icon from './Icon.svelte'
  import LyricsView from './LyricsView.svelte'
  import { store } from '../lib/state.svelte'
  import { videos } from '../lib/videos'
  import { follow } from '../lib/video'
  import { annotationsOf, frameClock, lyricExtras, lyricsOf } from '../lib/lyrics.svelte'
  import { beforeStart, clock, LYRICS_SOURCES, lyricsStatus, mayControl, positionMs, type MusicState } from '../lib/music'
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
  // Anyone may ask to skip; when the song isn't theirs it's a vote and half the room skips it.
  const votes = $derived(s.skipVotes?.length ?? 0)
  const voted = $derived(!!store.me && !!s.skipVotes?.includes(store.me.id))
  const source = $derived(s.lyrics ?? 'auto')
  // A music video's timing is the video's, not the song's: no lyrics for it, the queue instead.
  const lyrics = $derived(now.video ? null : lyricsOf(now, source))
  // Romanization and translation, asked only when this app wants one of them.
  const wantNotes = $derived(lyricExtras.roman || lyricExtras.translate)
  const notesRaw = $derived(wantNotes && lyrics?.lines.length ? annotationsOf(`${now.videoId}|${lyrics.source}`, lyrics.lines.map((l) => l.text)) : null)
  const notes = $derived(
    notesRaw ? notesRaw.map((a) => ({ roman: lyricExtras.roman ? a.roman : null, translation: lyricExtras.translate ? a.translation : null })) : null,
  )
  // A music video plays silently above the title, kept in step with the song everyone hears.
  let videoUrl = $state<string | null>(null)
  let videoReady = $state(false)
  let video: HTMLVideoElement | undefined = $state()
  // The video's own shape (4:3, 9:16, …), known once its metadata is in; 16:9 until then.
  let ratio = $state(16 / 9)
  function onMeta() {
    if (video?.videoWidth && video.videoHeight) ratio = video.videoWidth / video.videoHeight
  }

  // Ambient light, as on YouTube: the picture, shrunk to a few pixels, glows around the video. Each
  // sample is blended over the last, so the colours drift instead of flickering. Cheap on purpose:
  // a few samples a second (each one makes the GPU blur the glow again), none while paused or hidden,
  // and the stretched tiny canvas is already soft, so a light blur finishes it.
  let glow: HTMLCanvasElement | undefined = $state()
  $effect(() => {
    const v = video
    const c = glow
    if (!v || !c || !videoReady || paused) return
    const ctx = c.getContext('2d')
    if (!ctx) return
    const iv = setInterval(() => {
      if (v.readyState < 2 || document.hidden) return
      ctx.globalAlpha = 0.4
      ctx.drawImage(v, 0, 0, c.width, c.height)
    }, 400)
    return () => clearInterval(iv)
  })
  // Keyed on the id alone: every pause, resume or seek brings a new state object, and reloading the
  // video on each one dropped back to the thumbnail.
  const videoId = $derived(now.video ? now.videoId : null)
  $effect(() => {
    const id = videoId
    videoUrl = null
    videoReady = false
    ratio = 16 / 9
    if (!id) return
    let live = true
    videos.get(id).then((u) => { if (live) videoUrl = u }, () => {})
    return () => { live = false }
  })
  $effect(() => {
    const v = video
    if (v && videoReady) follow(v, pos, paused || beforeStart(s, music.receivedAt, frame.now))
  })

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
  {#if now.video}
    <!-- Opens at once with the video's thumbnail; the picture fades in over it when the first frame is in. -->
    <div class="vid ready">
      <div class="vcell">
      <canvas class="glow" class:on={videoReady} bind:this={glow} width="16" height={Math.max(1, Math.round(16 / ratio))} style="background-image:url('{now.artUrl}')" aria-hidden="true"></canvas>
      <div class="vbox" style="background-image:url('{now.artUrl}'); aspect-ratio: {ratio}">
        {#if videoUrl}<video
          class:on={videoReady}
          bind:this={video}
          src={videoUrl}
          muted
          playsinline
          preload="auto"
          disablepictureinpicture
          onloadedmetadata={onMeta}
          onloadeddata={() => (videoReady = true)}
          onerror={() => { videoUrl = null; videoReady = false }}
        ></video>{/if}
      </div>
      </div>
    </div>
  {/if}
  <div class="head" class:vmode={now.video}>
    <div class="sleeve">
      <div class="disc"><div class="label" style="background-image:url('{now.artUrl}')"></div></div>
      <div class="cover" style="background-image:url('{now.artUrl}')"></div>
    </div>
    <div class="info">
      {#if repeat === 'song'}<div class="kicker">On loop</div>{/if}
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
    <button
      class="skip"
      class:on={voted}
      title={canControl ? 'Skip' : voted ? `You voted to skip (${votes}/${s.skipNeeded})` : votes ? `Vote to skip (${votes}/${s.skipNeeded})` : `Vote to skip (${store.nick(now.requestedBy)} added it)`}
      onclick={() => store.musicCmd({ op: 'skip' })}
    >
      <Icon name="skip" />
      {#if votes}<span class="votes">{votes}/{s.skipNeeded}</span>{/if}
    </button>
    <button
      class:on={repeat !== 'off'}
      title={repeat === 'off' ? 'Loop: off (click for queue)' : repeat === 'queue' ? 'Loop: queue (click for song)' : 'Loop: this song (click to turn off)'}
      aria-label="Loop: {repeat}"
      onclick={() => store.musicCmd({ op: 'repeat', mode: null })}
    ><Icon name={repeat === 'song' ? 'repeatOne' : 'repeat'} /></button>
    {#if !now.video}<button class:on={store.queueOpen} title="Queue" onclick={() => (store.queueOpen = !store.queueOpen)}><Icon name="queue" /></button>{/if}
    {#if !now.video}<div class="src">
      <button
        class:on={picking}
        aria-haspopup="menu"
        aria-expanded={picking}
        title="Lyrics: {sourceName(source)}{source === 'auto' && lyrics?.source ? ` (${lyrics.source})` : ''}"
        onclick={() => (picking = !picking)}
      ><Icon name="lyrics" /></button>
      {#if picking}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="scrim" onmousedown={() => (picking = false)}></div>
        <div class="menu" role="menu" aria-label="Lyrics source">
          <div class="mhead">Lyrics from{source === 'auto' && lyrics?.source ? ` · now ${lyrics.source}` : ''}</div>
          {#each LYRICS_SOURCES as o (o.id)}
            <!-- Asking every source while the menu is open shows which ones know this song. -->
            {@const got = lyricsOf(now, o.id)}
            {@const st = lyricsStatus(got)}
            {@const inUse = o.id === source || (source === 'auto' && o.id !== 'auto' && lyrics?.source === o.name)}
            <button role="menuitemradio" aria-checked={o.id === source} class:on={o.id === source} disabled={!canPick && o.id !== source} onclick={() => canPick && pickSource(o.id)}>
              <span class="dot {st.tone}" aria-hidden="true"></span>
              <span class="nm">
                <b>{o.name}{#if inUse}<em>In use</em>{/if}</b>
                <small>{o.id === 'auto' && got ? `${got.source} · ` : ''}{st.label}</small>
              </span>
              {#if o.id === source}<Icon name="check" size={14} />{/if}
            </button>
          {/each}
          {#if !canPick}<div class="mnote">Only the channel's owner or an admin can change this</div>{/if}
          <div class="mhead extras">Under each line (just for you)</div>
          <label class="toggle"><span>Romanization</span><input type="checkbox" bind:checked={lyricExtras.roman} /></label>
          <label class="toggle"><span>Translate to English</span><input type="checkbox" bind:checked={lyricExtras.translate} /></label>
        </div>
      {/if}
    </div>{/if}
    <label class="vol" title="Music volume (only for you)">
      <Icon name="volume" size={16} />
      <Slider value={store.musicVolume} step={0.01} tone="light" label="Music volume" oninput={(v) => store.setMusicVolume(v)} />
    </label>
  </div>
  {#if s.queue.length}
    <div class="next">Up next: {s.queue[0].artist} — {s.queue[0].title}{s.queue.length > 1 ? ` · ${s.queue.length - 1} more` : ''}</div>
  {/if}

  <div class="below">
    {#if store.queueOpen || now.video}
      <div class="queue">
        <div class="add"><SongSearch glass autofocus={false} /></div>
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
      <LyricsView lines={lyrics.lines} {pos} {notes} />
    {:else if lyrics?.plain}
      <div class="plain"><pre>{lyrics.plain}</pre></div>
    {:else}
      <p class="none">{lyrics === undefined ? 'Looking for lyrics…' : 'No lyrics for this song'}</p>
    {/if}
  </div>
</aside>

<style>
  .player {
    flex: 1; width: 100%; min-height: 0; display: flex; flex-direction: column; gap: 12px; color: #fff;
    padding: 18px; border-radius: 16px; background: rgba(10, 10, 16, 0.32); backdrop-filter: blur(18px);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .vid { display: grid; grid-template-rows: 0fr; margin-bottom: -12px; opacity: 0; transition: grid-template-rows 650ms cubic-bezier(0.22, 1, 0.36, 1), margin 650ms cubic-bezier(0.22, 1, 0.36, 1), opacity 500ms ease-out; }
  .vid.ready { grid-template-rows: 1fr; margin-bottom: 0; opacity: 1; }
  .vcell { min-height: 0; position: relative; }
  /* Tall videos stop at 60% of the window; the picture keeps its shape, with black at the sides. */
  .vbox { position: relative; overflow: hidden; border-radius: 12px; max-height: 60vh; background: #000 center / cover no-repeat; }
  .glow {
    position: absolute; left: -5%; top: -6%; width: 110%; height: 112%; pointer-events: none;
    background: center / cover no-repeat; filter: blur(24px) saturate(1.5); opacity: 0.35; transition: opacity 800ms ease-out;
  }
  .glow.on { opacity: 0.6; }
  video { display: block; width: 100%; height: 100%; object-fit: contain; border-radius: 12px; opacity: 0; transition: opacity 400ms ease-out; }
  video.on { opacity: 1; }
  /* With a video playing, the record and sleeve fold away and the title sits under the picture. */
  .sleeve { transition: width 650ms cubic-bezier(0.22, 1, 0.36, 1), height 650ms cubic-bezier(0.22, 1, 0.36, 1), margin 650ms cubic-bezier(0.22, 1, 0.36, 1), opacity 300ms ease-out; }
  .head.vmode { gap: 0; }
  .head.vmode .sleeve { width: 0; height: 0; margin-right: 0; opacity: 0; overflow: hidden; }
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
  .kicker { font-size: 12px; font-weight: 600; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
  .ctl .skip { position: relative; }
  .votes {
    position: absolute; right: -6px; top: -6px; min-width: 22px; padding: 1px 5px; border-radius: 999px; font-size: 10px; font-weight: 800;
    color: #16171c; background: #fff; box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4); font-variant-numeric: tabular-nums;
  }
  .vol { flex: 1; display: flex; align-items: center; gap: 6px; margin-left: 4px; color: rgba(255, 255, 255, 0.8); min-width: 0; }
  .vol :global(.slider) { flex: 1; min-width: 40px; max-width: 120px; }
  .next { font-size: 12px; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .src { position: relative; }
  .scrim { position: fixed; inset: 0; z-index: 4; }
  .menu {
    position: absolute; top: calc(100% + 6px); right: -60px; z-index: 5; min-width: 240px; padding: 4px; border-radius: 10px;
    background: rgba(20, 20, 28, 0.92); backdrop-filter: blur(18px); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45), inset 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
  .src .menu button { width: 100%; height: auto; background: none; display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 7px 10px; border-radius: 7px; font-size: 13px; color: #fff; text-align: left; }
  .menu button:hover:not(:disabled) { background: rgba(255, 255, 255, 0.1); }
  .menu button:disabled { opacity: 0.45; cursor: default; }
  .mhead { padding: 6px 10px 4px; font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.6); }
  .mnote { padding: 6px 10px; font-size: 11.5px; color: rgba(255, 255, 255, 0.6); border-top: 1px solid rgba(255, 255, 255, 0.08); margin-top: 4px; }
  .mhead.extras { margin-top: 6px; border-top: 1px solid rgba(255, 255, 255, 0.08); padding-top: 10px; }
  .toggle { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 6px 10px; font-size: 13px; border-radius: 7px; cursor: pointer; }
  .toggle:hover { background: rgba(255, 255, 255, 0.08); }
  .toggle input { appearance: none; -webkit-appearance: none; width: 30px; height: 18px; border-radius: 999px; background: rgba(255, 255, 255, 0.2); position: relative; cursor: pointer; transition: background 150ms; margin: 0; }
  .toggle input::after { content: ''; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; transition: transform 150ms; }
  .toggle input:checked { background: #3ba55d; }
  .toggle input:checked::after { transform: translateX(12px); }
  .add { margin-bottom: 10px; }
  .add :global(.lists) { max-height: 260px; flex: none; }
  .menu button.on { background: rgba(255, 255, 255, 0.06); }
  .src .menu button { justify-content: flex-start; gap: 10px; }
  .dot { flex: none; width: 8px; height: 8px; border-radius: 50%; background: rgba(255, 255, 255, 0.35); }
  .dot.good { background: #3ba55d; box-shadow: 0 0 0 3px rgba(59, 165, 93, 0.25); }
  .dot.meh { background: #e3a53b; box-shadow: 0 0 0 3px rgba(227, 165, 59, 0.22); }
  .dot.bad { background: #ed4245; box-shadow: 0 0 0 3px rgba(237, 66, 69, 0.22); }
  .dot.wait { animation: wait 1s ease-in-out infinite; }
  @keyframes wait { 50% { opacity: 0.3; } }
  .nm { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .nm b { font-weight: 600; display: flex; align-items: center; gap: 6px; }
  .nm em { font-style: normal; font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; padding: 1px 6px; border-radius: 999px; color: #b8f5cb; background: rgba(59, 165, 93, 0.25); }
  .nm small { font-size: 11px; color: rgba(255, 255, 255, 0.6); }
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
    .vid, .sleeve { transition: none; }
    .disc, .plain pre, .dot.wait { animation: none; }
  }
</style>
