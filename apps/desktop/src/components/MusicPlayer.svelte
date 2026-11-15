<script lang="ts">
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'
  import { api } from '../lib/api'
  import { clock, lyricAt, mayControl, positionMs, type Lyrics, type MusicState } from '../lib/music'
  import { membersOf } from '../lib/voice'

  let { music }: { music: { state: MusicState; receivedAt: number } } = $props()
  const s = $derived(music.state)
  const now = $derived(s.now!)
  const paused = $derived(s.pausedAt !== null)

  // A clock that moves while playing (4×/s is enough for the bar and the lyrics).
  let t = $state(Date.now())
  $effect(() => {
    const iv = setInterval(() => (t = Date.now()), 250)
    return () => clearInterval(iv)
  })
  let dragMs = $state<number | null>(null)
  const pos = $derived(dragMs ?? positionMs(s, music.receivedAt, t))
  const pct = $derived(now.durationMs ? (pos / now.durationMs) * 100 : 0)

  const inVoice = $derived(membersOf(store.voice, s.channelId).map((m) => m.userId))
  const me = $derived({ id: store.me?.id ?? 0, admin: !!store.me?.isAdmin })
  const canControl = $derived(mayControl(now.requestedBy, me, inVoice))
  const onlyWho = $derived(`Only ${store.nick(now.requestedBy)} can control this song`)
  const status = $derived(store.musicStatus.get(now.videoId))

  // Lyrics for the playing song.
  let lyrics = $state<Lyrics | null>(null)
  let lyricsFor = $state('')
  $effect(() => {
    const e = now
    if (lyricsFor === e.videoId) return
    lyricsFor = e.videoId
    lyrics = null
    void api.musicLyrics(e.videoId, e.title, e.artist, e.durationMs).then((l) => {
      if (lyricsFor === e.videoId) lyrics = l
    }, () => {})
  })
  const line = $derived(lyrics?.lines.length ? lyricAt(lyrics.lines, pos + 150) : -1)
  let box: HTMLDivElement | undefined = $state()
  $effect(() => {
    const el = box?.querySelector<HTMLElement>(`[data-i="${line}"]`)
    if (box && el) box.scrollTo({ top: el.offsetTop - box.clientHeight / 2 + el.clientHeight / 2, behavior: 'smooth' })
  })

  function seekAt(e: PointerEvent, bar: HTMLElement) {
    const r = bar.getBoundingClientRect()
    return Math.round(Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * now.durationMs)
  }
  function down(e: PointerEvent) {
    if (!canControl) return
    const bar = e.currentTarget as HTMLElement
    bar.setPointerCapture(e.pointerId)
    dragMs = seekAt(e, bar)
  }
  function move(e: PointerEvent) {
    if (dragMs !== null) dragMs = seekAt(e, e.currentTarget as HTMLElement)
  }
  function up() {
    if (dragMs === null) return
    store.musicCmd({ op: 'seek', ms: dragMs })
    dragMs = null
  }
  function key(e: KeyboardEvent) {
    if (!canControl) return
    const d = e.key === 'ArrowRight' ? 5000 : e.key === 'ArrowLeft' ? -5000 : 0
    if (d) store.musicCmd({ op: 'seek', ms: Math.max(0, Math.min(now.durationMs - 1000, pos + d)) })
  }
</script>

<div class="player" class:paused>
  <div class="sleeve">
    <div class="disc"><div class="label" style="background-image:url('{now.artUrl}')"></div></div>
    <div class="cover" style="background-image:url('{now.artUrl}')"></div>
  </div>

  <div class="info">
    <div class="kicker">{paused ? 'Paused' : 'Now playing'} · requested by {store.nick(now.requestedBy)}</div>
    <div class="title" title={now.title}>{now.title}</div>
    <div class="artist">{now.artist}{now.album ? ` · ${now.album}` : ''}</div>
    {#if status?.status === 'error'}
      <div class="warn">Couldn't play this song here: {status.message}</div>
    {:else if status?.status === 'loading'}
      <div class="note">Loading the song…</div>
    {/if}
    <div class="seek">
      <span>{clock(pos)}</span>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="bar"
        class:locked={!canControl}
        role="slider"
        tabindex={canControl ? 0 : -1}
        aria-label="Song position"
        aria-valuemin={0}
        aria-valuemax={now.durationMs}
        aria-valuenow={pos}
        title={canControl ? undefined : onlyWho}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onkeydown={key}
      >
        <div class="fill" style="width:{pct}%"></div>
        <div class="knob" style="left:{pct}%"></div>
      </div>
      <span>{clock(now.durationMs)}</span>
    </div>
    <div class="ctl">
      <button class="big" disabled={!canControl} title={canControl ? (paused ? 'Resume' : 'Pause') : onlyWho} onclick={() => store.musicCmd({ op: paused ? 'resume' : 'pause' })}>
        <Icon name={paused ? 'play' : 'pause'} />
      </button>
      <button disabled={!canControl} title={canControl ? 'Skip' : onlyWho} onclick={() => store.musicCmd({ op: 'skip' })}><Icon name="skip" /></button>
      <button class:on={store.queueOpen} title="Queue" onclick={() => (store.queueOpen = !store.queueOpen)}><Icon name="queue" /></button>
      <label class="vol" title="Music volume (only for you)">
        <Icon name="volume" size={16} />
        <input type="range" min="0" max="1" step="0.05" value={store.musicVolume} oninput={(e) => store.setMusicVolume(Number(e.currentTarget.value))} />
      </label>
    </div>
    <div class="next">
      {#if s.queue.length}Up next: {s.queue[0].artist} — {s.queue[0].title}{s.queue.length > 1 ? ` · ${s.queue.length - 1} more` : ''}{:else}Nothing queued · type !p and a song{/if}
    </div>
  </div>

  {#if store.queueOpen}
    <div class="queue">
      <h3>Queue</h3>
      {#if !s.queue.length}<p class="empty">The queue is empty</p>{/if}
      <ol>
        {#each s.queue as e, i (e.id)}
          <li>
            <span class="qart" style="background-image:url('{e.artUrl}')"></span>
            <span class="qtxt"><b>{e.title}</b><small>{e.artist} · {clock(e.durationMs)} · {store.nick(e.requestedBy)}</small></span>
            {#if mayControl(e.requestedBy, me, inVoice)}
              <button title="Remove from the queue" onclick={() => store.musicCmd({ op: 'remove', index: i + 1 })}><Icon name="close" size={14} /></button>
            {/if}
          </li>
        {/each}
      </ol>
    </div>
  {:else if lyrics?.lines.length}
    <div class="lyrics" bind:this={box}>
      {#each lyrics.lines as l, i (i)}
        <p data-i={i} class:now={i === line} class:past={i < line}>
          {#if i === line && l.words.length}
            {#each l.words as w (w.ms)}<span class:sung={w.ms <= pos + 150}>{w.text} </span>{/each}
          {:else}{l.text || '♪'}{/if}
        </p>
      {/each}
    </div>
  {:else if lyrics?.plain}
    <div class="lyrics plain"><pre>{lyrics.plain}</pre></div>
  {:else}
    <div class="lyrics none"><p>{lyricsFor === now.videoId && lyrics === null ? 'No lyrics for this song' : ''}</p></div>
  {/if}
</div>

<style>
  .player { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(200px, 260px) minmax(240px, 1fr) minmax(0, 1fr); gap: 28px; align-items: center; color: #fff; padding: 4px 8px; }
  .sleeve { position: relative; width: 200px; height: 200px; justify-self: center; }
  .cover, .label { background-size: cover; background-position: center; background-color: #2a2b35; }
  .cover { position: absolute; inset: 0; border-radius: 10px; z-index: 2; box-shadow: 0 14px 40px rgba(0, 0, 0, 0.55); }
  .disc {
    position: absolute; z-index: 1; width: 186px; height: 186px; left: 68px; top: 7px; border-radius: 50%; display: grid; place-items: center;
    background: repeating-radial-gradient(circle, #111 0 2px, #1c1c22 2px 4px); box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
    animation: spin 6s linear infinite;
  }
  .paused .disc { animation-play-state: paused; }
  .label { width: 66px; height: 66px; border-radius: 50%; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .info { display: flex; flex-direction: column; gap: 9px; min-width: 0; }
  .kicker { text-transform: uppercase; letter-spacing: 0.08em; font-size: 10px; color: rgba(255, 255, 255, 0.75); }
  .title { font-weight: 800; font-size: 28px; line-height: 1.15; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .artist { color: rgba(255, 255, 255, 0.8); font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .warn { font-size: 12px; color: #ffb4b4; }
  .note { font-size: 12px; color: rgba(255, 255, 255, 0.7); }
  .seek { display: flex; align-items: center; gap: 8px; font-size: 11px; color: rgba(255, 255, 255, 0.75); font-variant-numeric: tabular-nums; }
  .bar { flex: 1; height: 16px; position: relative; cursor: pointer; touch-action: none; }
  .bar.locked { cursor: not-allowed; }
  .bar::before { content: ''; position: absolute; left: 0; right: 0; top: 6px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.25); }
  .fill { position: absolute; left: 0; top: 6px; height: 4px; border-radius: 2px; background: #fff; }
  .knob { position: absolute; top: 2px; width: 12px; height: 12px; border-radius: 50%; background: #fff; transform: translateX(-50%); opacity: 0; transition: opacity 120ms; }
  .bar:hover .knob, .bar:focus-visible .knob { opacity: 1; }
  .bar.locked .knob { display: none; }
  .ctl { display: flex; gap: 10px; align-items: center; }
  .ctl button { width: 38px; height: 38px; border-radius: 50%; background: rgba(255, 255, 255, 0.15); display: grid; place-items: center; color: #fff; }
  .ctl button:hover:not(:disabled) { background: rgba(255, 255, 255, 0.25); }
  .ctl button.big { background: #fff; color: #16171c; }
  .ctl button.on { background: rgba(255, 255, 255, 0.35); }
  .ctl button:disabled { opacity: 0.4; cursor: not-allowed; }
  .vol { display: flex; align-items: center; gap: 6px; margin-left: 6px; color: rgba(255, 255, 255, 0.8); }
  .vol input { width: 90px; accent-color: #fff; }
  .next { font-size: 12px; color: rgba(255, 255, 255, 0.75); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lyrics {
    align-self: stretch; overflow-y: auto; scrollbar-width: none; padding: 40% 0; font-size: 18px; font-weight: 700;
    mask-image: linear-gradient(transparent, #000 25%, #000 75%, transparent);
  }
  .lyrics p { margin: 0 0 12px; color: rgba(255, 255, 255, 0.38); transition: color 200ms, font-size 200ms; }
  .lyrics p.past { color: rgba(255, 255, 255, 0.28); }
  .lyrics p.now { color: rgba(255, 255, 255, 0.55); font-size: 22px; }
  .lyrics p.now:not(:has(span)) { color: #fff; }
  .lyrics p.now span.sung { color: #fff; }
  .lyrics.plain pre { margin: 0; font: inherit; font-size: 15px; white-space: pre-wrap; color: rgba(255, 255, 255, 0.7); animation: crawl 240s linear infinite; }
  @keyframes crawl { to { transform: translateY(-70%); } }
  .lyrics.none { display: grid; place-items: center; padding: 0; mask-image: none; }
  .lyrics.none p { color: rgba(255, 255, 255, 0.5); font-size: 14px; font-weight: 500; }
  .queue { align-self: stretch; overflow-y: auto; background: rgba(0, 0, 0, 0.3); backdrop-filter: blur(8px); border-radius: 12px; padding: 12px; }
  .queue h3 { margin: 0 0 8px; font-size: 13px; text-transform: uppercase; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.75); }
  .queue ol { list-style: none; margin: 0; padding: 0; }
  .queue li { display: flex; align-items: center; gap: 10px; padding: 6px; border-radius: 8px; }
  .queue li:hover { background: rgba(255, 255, 255, 0.08); }
  .qart { width: 36px; height: 36px; border-radius: 6px; background-size: cover; background-color: #2a2b35; flex: none; }
  .qtxt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .qtxt b, .qtxt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .qtxt small { color: rgba(255, 255, 255, 0.65); }
  .queue button { color: rgba(255, 255, 255, 0.7); display: grid; padding: 4px; border-radius: 6px; }
  .queue button:hover { color: #fff; background: rgba(255, 255, 255, 0.12); }
  .empty { color: rgba(255, 255, 255, 0.6); font-size: 13px; }
  @media (max-width: 900px) {
    .player { grid-template-columns: 170px 1fr; }
    .sleeve { transform: scale(0.8); }
    .lyrics, .queue { display: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .disc, .lyrics.plain pre { animation: none; }
  }
</style>
