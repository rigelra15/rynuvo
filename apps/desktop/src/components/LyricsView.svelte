<script lang="ts">
  import { lyricAt, wordFill, type LyricLine } from '../lib/music'

  /** `pos`: where the song is now (ms), updated every frame while playing. */
  let { lines, pos }: { lines: LyricLine[]; pos: number } = $props()

  // Lines come in a touch early, the way a singer's breath does.
  const at = $derived(pos + 150)
  const line = $derived(lyricAt(lines, at))
  const nextMs = $derived(lines[line + 1]?.ms ?? Infinity)

  // The list glides (transform only, on the compositor) so the current line sits a bit above centre.
  let list: HTMLDivElement | undefined = $state()
  let vh = $state(0)
  let offset = $state(0)
  $effect(() => {
    const i = Math.max(0, line)
    void vh
    const el = list?.querySelector<HTMLElement>(`[data-i="${i}"]`)
    if (el) offset = vh * 0.4 - (el.offsetTop + el.offsetHeight / 2)
  })
</script>

<div class="view" bind:clientHeight={vh}>
  <div class="list" bind:this={list} style="transform:translate3d(0,{offset}px,0)">
    {#each lines as l, i (i)}
      {@const d = Math.abs(i - line)}
      <p data-i={i} class:now={i === line} class:past={i < line} style="--d:{Math.min(d, 4)}">
        {#if i === line && l.words.length}
          {#each l.words as w, j (j)}{#if j}{' '}{/if}<span class="w" style="--p:{(wordFill(l, j, at, nextMs) * 100).toFixed(1)}%">{w.text}</span>{/each}
        {:else}{l.text || '♪'}{/if}
      </p>
    {/each}
  </div>
</div>

<style>
  .view {
    position: relative; overflow: hidden; height: 100%;
    mask-image: linear-gradient(transparent, #000 18%, #000 72%, transparent);
  }
  .list { padding: 0 4px; will-change: transform; transition: transform 700ms cubic-bezier(0.22, 1, 0.36, 1); }
  p {
    margin: 0; padding: 7px 0; font-size: 22px; font-weight: 700; line-height: 1.3; color: #fff;
    transform-origin: left center; transform: scale(0.94);
    opacity: calc(0.5 - var(--d) * 0.08);
    transition: transform 500ms cubic-bezier(0.22, 1, 0.36, 1), opacity 500ms ease;
  }
  p.past { opacity: calc(0.42 - var(--d) * 0.08); }
  p.now { transform: scale(1); opacity: 1; }
  /* Word by word: each word fills with white as it is sung. */
  .w {
    background: linear-gradient(90deg, #fff var(--p), rgba(255, 255, 255, 0.42) var(--p));
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  @media (prefers-reduced-motion: reduce) {
    .list, p { transition: none; }
  }
</style>
