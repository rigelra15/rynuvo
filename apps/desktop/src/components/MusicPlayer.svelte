<script lang="ts">
  import Icon from './Icon.svelte'
  import LyricsView from './LyricsView.svelte'
  import { store } from '../lib/state.svelte'
  import { frameClock, lyricsOf } from '../lib/lyrics.svelte'
  import { clock, mayControl, positionMs, type MusicState } from '../lib/music'
  import { membersOf } from '../lib/voice'

  let { music }: { music: { state: MusicState; receivedAt: number } } = $props()
  const s = $derived(music.state)
  const now = $derived(s.now!)
  const paused = $derived(s.pausedAt !== null)

  // Every frame while playing, so the bar and the lyrics glide instead of stepping.
  const frame = frameClock(() => !paused)
  let dragMs = $state<number | null>(null)
  const pos = $derived(dragMs ?? positionMs(s, music.receivedAt, frame.now))
  const pct = $derived(now.durationMs ? (pos / now.durationMs) * 100 : 0)

  const inVoice = $derived(membersOf(store.voice, s.channelId).map((m) => m.userId))
  const me = $derived({ id: store.me?.id ?? 0, admin: !!store.me?.isAdmin })
  const canControl = $derived(mayControl(now.requestedBy, me, inVoice))
  const onlyWho = $derived(`Only ${store.nick(now.requestedBy)} can control this song`)
  const status = $derived(store.musicStatus.get(now.videoId))
  const lyrics = $derived(lyricsOf(now))

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

<aside class="player" class:paused>
  <div class="head">
    <div class="sleeve">
      <div class="disc"><div class="label" style="background-image:url('{now.artUrl}')"></div></div>
      <div class="cover" style="background-image:url('{now.artUrl}')"></div>
    </div>
    <div class="info">
      <div class="kicker">{paused ? 'Paused' : 'Now playing'} · {store.nick(now.requestedBy)}</div>
      <div class="title" title={now.title}>{now.title}</div>
      <div class="artist">{now.artist}{now.album ? ` · ${now.album}` : ''}</div>
    </div>
  </div>
  {#if status?.status === 'error'}
    <div class="warn">Couldn't play this song here: {status.message}</div>
  {:else if status?.status === 'loading'}
    <div class="note">Loading the song…</div>
  {/if}
  <div class="seek">
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
      <div class="fill" style="transform:scaleX({pct / 100})"></div>
      <div class="knob" style="left:{pct}%"></div>
    </div>
    <div class="times"><span>{clock(pos)}</span><span>{clock(now.durationMs)}</span></div>
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

  <div class="below">
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
  .sleeve { position: relative; flex: none; width: 112px; height: 112px; margin-right: 26px; }
  .cover, .label { background-size: cover; background-position: center; background-color: #2a2b35; }
  .cover { position: absolute; inset: 0; border-radius: 10px; z-index: 2; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5); }
  .disc {
    position: absolute; z-index: 1; width: 104px; height: 104px; left: 34px; top: 4px; border-radius: 50%; display: grid; place-items: center;
    background: repeating-radial-gradient(circle, #111 0 2px, #1c1c22 2px 4px); box-shadow: 0 8px 22px rgba(0, 0, 0, 0.5);
    animation: spin 6s linear infinite;
  }
  .paused .disc { animation-play-state: paused; }
  .label { width: 38px; height: 38px; border-radius: 50%; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .info { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .kicker { text-transform: uppercase; letter-spacing: 0.08em; font-size: 10px; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 800; font-size: 22px; line-height: 1.15; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .artist { color: rgba(255, 255, 255, 0.78); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .warn { font-size: 12px; color: #ffb4b4; }
  .note { font-size: 12px; color: rgba(255, 255, 255, 0.7); }
  .seek { display: flex; flex-direction: column; gap: 2px; }
  .times { display: flex; justify-content: space-between; font-size: 11px; color: rgba(255, 255, 255, 0.7); font-variant-numeric: tabular-nums; }
  .bar { height: 16px; position: relative; cursor: pointer; touch-action: none; }
  .bar.locked { cursor: not-allowed; }
  .bar::before { content: ''; position: absolute; left: 0; right: 0; top: 6px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.22); }
  .fill { position: absolute; left: 0; right: 0; top: 6px; height: 4px; border-radius: 2px; background: #fff; transform-origin: left; }
  .knob { position: absolute; top: 2px; width: 12px; height: 12px; border-radius: 50%; background: #fff; transform: translateX(-50%); opacity: 0; transition: opacity 120ms; }
  .bar:hover .knob, .bar:focus-visible .knob { opacity: 1; }
  .bar.locked .knob { display: none; }
  .ctl { display: flex; gap: 10px; align-items: center; }
  .ctl button { width: 38px; height: 38px; border-radius: 50%; background: rgba(255, 255, 255, 0.14); display: grid; place-items: center; color: #fff; transition: background 120ms, transform 120ms; }
  .ctl button:hover:not(:disabled) { background: rgba(255, 255, 255, 0.24); }
  .ctl button:active:not(:disabled) { transform: scale(0.94); }
  .ctl button.big { width: 44px; height: 44px; background: #fff; color: #16171c; }
  .ctl button.on { background: rgba(255, 255, 255, 0.34); }
  .ctl button:disabled { opacity: 0.4; cursor: not-allowed; }
  .vol { flex: 1; display: flex; align-items: center; gap: 6px; margin-left: 4px; color: rgba(255, 255, 255, 0.8); min-width: 0; }
  .vol input { flex: 1; min-width: 40px; max-width: 120px; accent-color: #fff; }
  .next { font-size: 12px; color: rgba(255, 255, 255, 0.7); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .below { flex: 1; min-height: 0; margin-top: 4px; border-top: 1px solid rgba(255, 255, 255, 0.08); padding-top: 6px; }
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
  .queue button { color: rgba(255, 255, 255, 0.7); display: grid; padding: 4px; border-radius: 6px; }
  .queue button:hover { color: #fff; background: rgba(255, 255, 255, 0.12); }
  .empty { color: rgba(255, 255, 255, 0.6); font-size: 13px; }
  @media (prefers-reduced-motion: reduce) {
    .disc, .plain pre { animation: none; }
  }
</style>
