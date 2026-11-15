<script lang="ts">
  // The music panel when nothing is playing: a still record and a search to put a song on.
  import SongSearch from './SongSearch.svelte'

  let { channelName }: { channelName: string } = $props()
</script>

<aside class="idle" aria-label="Music">
  <div class="head">
    <div class="sleeve">
      <div class="disc"><div class="label"></div></div>
      <div class="cover"><span>♪</span></div>
    </div>
    <div class="info">
      <div class="title">Nothing playing</div>
      <div class="sub">Put a song on. Everyone in {channelName} hears it together.</div>
    </div>
  </div>
  <div class="find">
    <SongSearch glass autofocus={false} />
  </div>
  <p class="tip">Or type <code>!p</code> and a song in the chat</p>
</aside>

<style>
  .idle {
    flex: 1; width: 100%; min-height: 0; display: flex; flex-direction: column; gap: 16px; color: #fff;
    padding: 18px; border-radius: 16px; background: rgba(10, 10, 16, 0.32); backdrop-filter: blur(18px);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .head { display: flex; gap: 16px; align-items: center; min-width: 0; }
  .sleeve { position: relative; flex: none; width: 112px; height: 112px; margin-right: 34px; }
  .cover {
    position: absolute; inset: 0; z-index: 2; border-radius: 10px; display: grid; place-items: center;
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.14), rgba(255, 255, 255, 0.04));
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12), 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  .cover span { font-size: 40px; color: rgba(255, 255, 255, 0.55); }
  /* The record peeks a little out of its sleeve, waiting. */
  .disc {
    position: absolute; z-index: 1; width: 104px; height: 104px; left: 38px; top: 4px; border-radius: 50%; display: grid; place-items: center;
    background: repeating-radial-gradient(circle, #111 0 2px, #1c1c22 2px 4px); box-shadow: 0 8px 22px rgba(0, 0, 0, 0.45);
  }
  .label { position: relative; width: 40px; height: 40px; border-radius: 50%; background: var(--accent); opacity: 0.85; }
  .label::after { content: ''; position: absolute; left: 50%; top: 50%; width: 6px; height: 6px; margin: -3px; border-radius: 50%; background: #111; }
  .info { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .title { font-weight: 800; font-size: 22px; line-height: 1.15; }
  .sub { color: rgba(255, 255, 255, 0.75); font-size: 13px; line-height: 1.4; }
  .find { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .find :global(.ss) { flex: 1; }
  .tip { margin: 0; font-size: 12px; color: rgba(255, 255, 255, 0.55); }
  .tip code { font: inherit; font-weight: 700; color: rgba(255, 255, 255, 0.8); }
</style>
