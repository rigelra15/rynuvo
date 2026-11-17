<script lang="ts">
  import { onMount } from 'svelte'
  import { fade } from 'svelte/transition'
  import { errText } from '../lib/api'

  // Scanning the transfer QR another device shows in Settings. The camera runs behind the page
  // (the plugin's windowed mode), so the page goes see-through and this draws over it: the room
  // dims, a diamond window shows where to aim, a line sweeps across it, and it turns green on a hit.
  let { onCode, onClose }: { onCode: (text: string) => void; onClose: (error?: string) => void } = $props()

  let live = $state(false)
  let found = $state(false)
  let over = false

  function body(node: HTMLElement) {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }

  async function stop() {
    if (over) return
    over = true
    const { cancel } = await import('@tauri-apps/plugin-barcode-scanner')
    await cancel().catch(() => {})
  }

  onMount(() => {
    document.documentElement.classList.add('qr-scanning')
    // Back (the gesture or the button) cancels the scan, through history like the rest of the app.
    history.pushState({ qr: true }, '')
    const back = () => {
      void stop()
      onClose()
    }
    window.addEventListener('popstate', back)
    // Android's Back asks the page first (MainActivity): close the scanner rather than leave the app.
    const w = window as unknown as { __rynuvoBack?: () => boolean }
    w.__rynuvoBack = () => {
      cancelTap()
      return true
    }

    void (async () => {
      try {
        const { scan, Format, checkPermissions, requestPermissions } = await import('@tauri-apps/plugin-barcode-scanner')
        if ((await checkPermissions()) !== 'granted' && (await requestPermissions()) !== 'granted') {
          return finish(undefined, 'Camera access is needed to scan the code.')
        }
        live = true
        const r = await scan({ windowed: true, formats: [Format.QRCode] })
        over = true
        found = true
        navigator.vibrate?.(30)
        // A beat of green before signing in.
        setTimeout(() => finish(r.content), 420)
      } catch (e) {
        if (!over) finish(undefined, errText(e))
      }
    })()

    function finish(code?: string, error?: string) {
      window.removeEventListener('popstate', back)
      if (history.state?.qr) history.back()
      if (code !== undefined) onCode(code)
      else onClose(error)
    }

    return () => {
      if (w.__rynuvoBack) delete w.__rynuvoBack
      window.removeEventListener('popstate', back)
      document.documentElement.classList.remove('qr-scanning')
      void stop()
    }
  })

  function cancelTap() {
    // Same as Back: popping our history entry cancels and closes.
    if (history.state?.qr) history.back()
    else {
      void stop()
      onClose()
    }
  }
</script>

<div class="scanner" class:live class:found use:body transition:fade={{ duration: 260 }} role="dialog" aria-label="Scan the transfer QR">
  <div class="top">
    <h2>Scan to sign in</h2>
    <p>On your other device, open <b>Settings → Transfer code</b>, then point the camera at the QR.</p>
  </div>

  <div class="aim" aria-hidden="true">
    <div class="hole"></div>
    <div class="sweep"><i></i></div>
  </div>

  <div class="bottom">
    <span class="state">{found ? 'Got it' : live ? 'Looking for the code…' : 'Starting the camera…'}</span>
    <button class="cancel" onclick={cancelTap}>Cancel</button>
  </div>
</div>

<style>
  /* While scanning the camera is behind the WebView: let it show through. */
  :global(html.qr-scanning),
  :global(html.qr-scanning body),
  :global(html.qr-scanning #app) { background: transparent !important; }
  :global(html.qr-scanning #app) { visibility: hidden; }

  .scanner {
    --s: min(64vw, 270px);
    position: fixed; inset: 0; z-index: 2000; display: flex; flex-direction: column; align-items: center; justify-content: space-between;
    padding: calc(env(safe-area-inset-top) + 28px) 24px calc(env(safe-area-inset-bottom) + 24px); color: #fff; overflow: hidden;
  }
  /* Before the camera is up the room stays dark, so nothing flashes through. */
  .scanner:not(.live) { background: var(--desk); }

  .top { position: relative; z-index: 2; text-align: center; max-width: 340px; }
  h2 { margin: 0 0 8px; font-size: 26px; font-weight: 750; font-stretch: 80%; letter-spacing: -0.01em; }
  .top p { margin: 0; font-size: 14.5px; line-height: 1.5; color: rgba(255, 255, 255, 0.78); }
  .top b { color: var(--accent); font-weight: 650; }

  .aim { position: absolute; left: 50%; top: 50%; width: calc(var(--s) * 1.414); height: calc(var(--s) * 1.414); transform: translate(-50%, -50%); display: grid; place-items: center; }
  /* A rounded square turned into a diamond; its huge shadow dims everything around it. */
  .hole {
    width: var(--s); height: var(--s); border-radius: 30px; transform: rotate(45deg) scale(1.25); opacity: 0;
    border: 3px solid var(--accent); box-shadow: 0 0 0 200vmax rgba(12, 8, 16, 0.66), 0 0 26px rgba(244, 184, 96, 0.55), inset 0 0 26px rgba(244, 184, 96, 0.25);
    transition: transform 620ms cubic-bezier(0.22, 1, 0.36, 1), opacity 300ms, border-color 240ms, box-shadow 240ms;
  }
  .live .hole { transform: rotate(45deg) scale(1); opacity: 1; animation: breathe 2.6s 700ms ease-in-out infinite; }
  .found .hole { border-color: var(--speaking); box-shadow: 0 0 0 200vmax rgba(12, 8, 16, 0.66), 0 0 34px rgba(74, 222, 128, 0.8), inset 0 0 40px rgba(74, 222, 128, 0.45); animation: none; transform: rotate(45deg) scale(0.94); }
  @keyframes breathe { 50% { transform: rotate(45deg) scale(1.03); } }

  /* The sweep, cut to the diamond. */
  .sweep { position: absolute; inset: 6%; clip-path: polygon(50% 0, 100% 50%, 50% 100%, 0 50%); opacity: 0; transition: opacity 300ms 400ms; }
  .live .sweep { opacity: 1; }
  .found .sweep { opacity: 0; transition-delay: 0ms; }
  .sweep i {
    position: absolute; left: 0; right: 0; height: 36%; top: -36%;
    background: linear-gradient(to bottom, transparent, rgba(244, 184, 96, 0.22) 80%, rgba(255, 214, 144, 0.95) 98%, transparent);
    animation: sweep 2.2s cubic-bezier(0.65, 0, 0.35, 1) infinite;
  }
  @keyframes sweep { to { top: 100%; } }

  .bottom { position: relative; z-index: 2; width: 100%; max-width: 360px; display: flex; flex-direction: column; align-items: center; gap: 14px; }
  .state { font-size: 13px; font-weight: 600; color: rgba(255, 255, 255, 0.7); transition: color 240ms; }
  .found .state { color: var(--speaking); }
  .cancel {
    width: 100%; height: 52px; border-radius: 16px; font-size: 16px; font-weight: 700; color: #fff;
    background: rgba(255, 255, 255, 0.12); border: 1px solid rgba(255, 255, 255, 0.18); backdrop-filter: blur(14px); -webkit-backdrop-filter: blur(14px);
  }
  .cancel:active { transform: scale(0.98); }

  @media (prefers-reduced-motion: reduce) {
    .live .hole { animation: none; }
    .sweep i { animation: none; top: 40%; }
  }
</style>
