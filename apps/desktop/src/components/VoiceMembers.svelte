<script lang="ts">
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import type { Id } from '../lib/types'
  import Icon from './Icon.svelte'
  import { fade } from 'svelte/transition'
  import { lyricsOf } from '../lib/lyrics.svelte'
  import { lyricAt, positionMs } from '../lib/music'

  let { channelId, onMenu }: { channelId: Id; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channelId))
  const music = $derived(store.musicIn(channelId))
  const now = $derived(music?.state.now ?? null)
  const paused = $derived(music?.state.pausedAt != null)

  // A calm clock is enough here: the line changes every few seconds, the bar moves slowly.
  let t = $state(Date.now())
  $effect(() => {
    if (!now || paused) return
    const iv = setInterval(() => (t = Date.now()), 300)
    return () => clearInterval(iv)
  })
  const pos = $derived(music ? positionMs(music.state, music.receivedAt, t) : 0)
  const lyrics = $derived(now ? lyricsOf(now, music?.state.lyrics ?? 'auto') : undefined)
  const lineAt = $derived(lyrics?.lines.length ? lyricAt(lyrics.lines, pos + 150) : -1)
  const lyric = $derived(lineAt >= 0 ? lyrics!.lines[lineAt].text : '')
</script>

{#if now}
  <div class="card" class:paused title="{now.title} — {now.artist}">
    <span class="art" style="background-image:url('{now.artUrl}')"><span class="eq" aria-hidden="true"><i></i><i></i><i></i></span></span>
    <span class="txt">
      <b>{now.title}</b>
      <span class="sub">
        {#key lyric || now.artist}<span class="ln" class:lyric={!!lyric} in:fade={{ duration: 260 }}>{lyric || now.artist}</span>{/key}
      </span>
    </span>
    <span class="prog" style="transform:scaleX({now.durationMs ? pos / now.durationMs : 0})"></span>
  </div>
{/if}
{#if list.length}
  <ul class="vm">
    {#each list as m (m.userId)}
      <li oncontextmenu={(e) => { e.preventDefault(); onMenu(m.userId, e) }}>
        <Avatar user={store.user(m.userId)} id={m.userId} size={20} speaking={store.speaking.has(m.userId)} />
        <span class="n">{store.nick(m.userId)}</span>
        {#if m.deafened}<span class="st" title="Deafened"><Icon name="headphonesOff" size={13} /></span>
        {:else if m.muted}<span class="st" title="Muted"><Icon name="micOff" size={13} /></span>{/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .card {
    position: relative; overflow: hidden; margin: 2px 6px 4px 28px; padding: 7px 8px; border-radius: 10px; display: flex; gap: 9px; align-items: center;
    background: color-mix(in srgb, var(--accent) 14%, var(--elev-2)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .art { position: relative; flex: none; width: 34px; height: 34px; border-radius: 7px; background-size: cover; background-position: center; background-color: var(--elev-2); }
  .eq { position: absolute; right: 2px; bottom: 2px; display: flex; gap: 1.5px; align-items: flex-end; height: 10px; padding: 1px 2px; border-radius: 3px; background: rgba(0, 0, 0, 0.55); }
  .eq i { width: 2px; height: 100%; background: #fff; border-radius: 1px; transform-origin: bottom; animation: eq 900ms ease-in-out infinite alternate; }
  .eq i:nth-child(2) { animation-delay: -300ms; }
  .eq i:nth-child(3) { animation-delay: -600ms; }
  .paused .eq i { animation-play-state: paused; }
  @keyframes eq { from { transform: scaleY(0.25); } to { transform: scaleY(1); } }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .txt b { font-size: 12.5px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { display: grid; font-size: 11.5px; }
  .ln { grid-area: 1 / 1; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ln.lyric { color: color-mix(in srgb, var(--accent) 55%, var(--text)); font-style: italic; }
  .prog { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: var(--accent); transform-origin: left; transition: transform 300ms linear; }
  @media (prefers-reduced-motion: reduce) { .eq i { animation: none; } }
  .vm { list-style: none; margin: 0 0 4px; padding: 0 0 0 28px; }
  li { display: flex; align-items: center; gap: 7px; padding: 3px 6px; border-radius: 6px; color: var(--muted); font-size: 13px; }
  li:hover { background: var(--elev-2); color: var(--text); }
  .n { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st { color: var(--faint); display: grid; }
</style>
