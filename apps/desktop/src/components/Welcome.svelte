<script lang="ts">
  import { cubicIn, cubicOut } from 'svelte/easing'
  import { fade, type TransitionConfig } from 'svelte/transition'
  import Icon from './Icon.svelte'
  import { permissions, type Perm } from '../lib/permissions.svelte'

  // First launch on a phone, between the splash and signing in: hello, then the three things the
  // app needs from Android, each with a small picture of what it's for, then off to sign in.
  let { signedIn = false, onDone }: { signedIn?: boolean; onDone: () => void } = $props()

  const steps = ['hello', 'mic', 'notifications', 'install', 'done'] as const
  type Step = (typeof steps)[number]
  let at = $state(0)
  /** +1 going forward, -1 back: which way the pictures slide. */
  let dir = 1
  const step = $derived<Step>(steps[at])
  const perm = $derived<Perm | null>(step === 'mic' || step === 'notifications' || step === 'install' ? step : null)
  const status = $derived(perm ? permissions.state[perm] : null)

  function go(to: number) {
    if (to < 0 || to >= steps.length || to === at) return
    dir = to > at ? 1 : -1
    at = to
    askedHere = false
  }
  const next = () => (at === steps.length - 1 ? onDone() : go(at + 1))

  // Android's answer comes back on its own (a dialog), or when the user returns from Settings: keep
  // reading where things stand while this is up.
  $effect(() => {
    permissions.refresh()
    const t = setInterval(() => permissions.refresh(), 700)
    const seen = () => document.visibilityState === 'visible' && permissions.refresh()
    document.addEventListener('visibilitychange', seen)
    return () => {
      clearInterval(t)
      document.removeEventListener('visibilitychange', seen)
    }
  })

  // Allowed just now from this step: a beat to see the tick, then on to the next.
  let askedHere = $state(false)
  $effect(() => {
    if (!askedHere || status !== 'granted') return
    const t = setTimeout(next, 1100)
    return () => clearTimeout(t)
  })

  function primary() {
    if (!perm || status === 'granted') return next()
    askedHere = true
    permissions.ask(perm)
  }

  const copy: Record<Step, { title: string; text: string }> = $derived({
    hello: {
      title: 'Welcome to Rynuvo',
      text: 'A small place for your circle: rooms to talk and hang out, chats, and music everyone hears together. Three quick things, then you’re in.',
    },
    mic: {
      title: 'Let them hear you',
      text: 'Rooms use your microphone so friends can hear you. It’s only on while you’re in a room, and you can mute any time.',
    },
    notifications: {
      title: 'Stay in the call',
      text: 'While you’re in a room, a notification keeps the call going in the background, with mute and leave right there.',
    },
    install: {
      title: 'Updates in one tap',
      text: 'Rynuvo updates itself. Android needs your OK once to let it install them: switch it on for Rynuvo in the next screen, then come back here.',
    },
    done: {
      title: 'You’re all set',
      text: signedIn ? 'That’s everything. Back to your circle.' : 'That’s everything. Now sign in with the invite code a friend gave you.',
    },
  })

  const label = $derived.by(() => {
    if (step === 'hello') return 'Get started'
    if (step === 'done') return signedIn ? 'Let’s go' : 'Sign in'
    if (status === 'granted') return 'Continue'
    if (step === 'install' || status === 'blocked') return 'Open settings'
    return step === 'mic' ? 'Allow microphone' : 'Allow notifications'
  })
  const blockedHint = $derived(
    status === 'blocked' && step !== 'install'
      ? `Android won’t ask again. In Settings, open Permissions → ${step === 'mic' ? 'Microphone' : 'Notifications'} and allow it.`
      : null,
  )

  const summary: { p: Perm; name: string }[] = [
    { p: 'mic', name: 'Microphone' },
    { p: 'notifications', name: 'Notifications' },
    { p: 'install', name: 'Updates' },
  ]

  // Swipe between steps, like flipping cards.
  let x0: number | null = null
  const down = (e: PointerEvent) => (x0 = e.clientX)
  function up(e: PointerEvent) {
    if (x0 === null) return
    const dx = e.clientX - x0
    x0 = null
    if (dx < -60) next()
    else if (dx > 60) go(at - 1)
  }

  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
  /** A picture or a line of text, sliding in from the side it's going to. */
  function slide(node: Element, { delay = 0, out = false }: { delay?: number; out?: boolean } = {}): TransitionConfig {
    const d = dir
    return {
      delay: reduced ? 0 : delay,
      duration: reduced ? 120 : out ? 220 : 520,
      easing: out ? cubicIn : cubicOut,
      css: (t, u) =>
        reduced ? `opacity:${t}` : `opacity:${t};transform:translateX(${(out ? -d : d) * u * 48}px) scale(${0.94 + 0.06 * t})`,
    }
  }
  /** Leaving for sign-in: the whole thing lifts away and dissolves. */
  function leave(_node: Element): TransitionConfig {
    return { duration: reduced ? 150 : 560, easing: cubicIn, css: (t, u) => `opacity:${t};transform:scale(${1 + u * 0.05});filter:blur(${(u * 8).toFixed(1)}px)` }
  }
</script>

<div class="welcome" out:leave role="dialog" aria-label="Welcome to Rynuvo">
  <div class="bloom warm" style="--x:{20 + at * 14}%"></div>
  <div class="bloom lilac" style="--x:{80 - at * 12}%"></div>

  <header class="bar">
    <div class="dots" aria-hidden="true">
      {#each steps as s, i (s)}<span class:on={i <= at} class:here={i === at}></span>{/each}
    </div>
    {#if at > 0 && step !== 'done'}
      <button class="skip" onclick={() => go(steps.length - 1)}>Skip</button>
    {/if}
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="stage" onpointerdown={down} onpointerup={up} onpointercancel={() => (x0 = null)}>
    {#key step}
      <div class="pic" in:slide out:slide={{ out: true }}>
        {#if step === 'hello'}
          <div class="hello">
            <div class="ring r1"><span style="--c:var(--lilac)"></span><span style="--c:var(--speaking)"></span></div>
            <div class="ring r2"><span style="--c:var(--accent)"></span><span style="--c:#7dd3fc"></span><span style="--c:#f9a8d4"></span></div>
            <svg class="gem" viewBox="-64 -64 128 128" aria-hidden="true">
              <path d="M0 -58 L-58 0 L0 0 Z" fill="#ffd690" />
              <path d="M0 -58 L58 0 L0 0 Z" fill="#f4b860" />
              <path d="M58 0 L0 58 L0 0 Z" fill="#c98a3a" />
              <path d="M-58 0 L0 58 L0 0 Z" fill="#e0a24f" />
            </svg>
          </div>
        {:else if step === 'mic'}
          <div class="mic" class:ok={status === 'granted'}>
            <span class="wave w1"></span><span class="wave w2"></span><span class="wave w3"></span>
            <div class="disc"><Icon name="mic" size={46} /></div>
            {#if status === 'granted'}<span class="tick"><Icon name="check" size={18} /></span>{/if}
            <div class="eq" aria-hidden="true">{#each Array(11) as _, i}<i style="--i:{i}"></i>{/each}</div>
          </div>
        {:else if step === 'notifications'}
          <div class="notif" class:ok={status === 'granted'}>
            <div class="phone">
              <div class="sb"><span>9:41</span><span class="sbi"></span></div>
              <div class="card">
                <span class="nic"><svg viewBox="-12 -12 24 24"><path d="M0 -9 L9 0 L0 9 L-9 0 Z" fill="currentColor" /></svg></span>
                <div class="nt"><b>Music Room</b><small>In a call · 12:04</small></div>
                <div class="nb"><span>Mute</span><span>Leave</span></div>
              </div>
            </div>
            <div class="bell">
              <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
                <path d="M6 9a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 15 6 9" /><path d="M10 20a2.2 2.2 0 0 0 4 0" />
              </svg>
              {#if status === 'granted'}<span class="tick"><Icon name="check" size={14} /></span>{/if}
            </div>
          </div>
        {:else if step === 'install'}
          <div class="inst" class:ok={status === 'granted'}>
            <svg class="outline" viewBox="-70 -70 140 140" aria-hidden="true">
              <path class="track" d="M0 -60 L60 0 L0 60 L-60 0 Z" pathLength="100" />
              <path class="fill" d="M0 -60 L60 0 L0 60 L-60 0 Z" pathLength="100" />
            </svg>
            <div class="arrow">
              {#if status === 'granted'}<Icon name="check" size={40} />{:else}
                <svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v12" /><path d="m6 11 6 6 6-6" /><path d="M5 20h14" /></svg>
              {/if}
            </div>
            <span class="chip">New version</span>
          </div>
        {:else}
          <div class="done">
            <svg class="gem small" viewBox="-64 -64 128 128" aria-hidden="true">
              <path d="M0 -58 L58 0 L0 58 L-58 0 Z" fill="url(#wg)" />
              <path class="ck" d="M-22 2 L-6 18 L24 -14" fill="none" stroke="#2a1a05" stroke-width="9" stroke-linecap="round" stroke-linejoin="round" pathLength="1" />
              <defs><linearGradient id="wg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffd690" /><stop offset="1" stop-color="#d9973f" /></linearGradient></defs>
            </svg>
            <ul>
              {#each summary as s, i (s.p)}
                {@const ok = permissions.state[s.p] === 'granted'}
                <li style="--i:{i}" class:ok>
                  <span>{s.name}</span>
                  {#if ok}<b><Icon name="check" size={13} /> Allowed</b>{:else}<button onclick={() => go(i + 1)}>Not yet</button>{/if}
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/key}
  </div>

  <div class="copy">
    {#key step}
      <div class="lines" in:slide={{ delay: 70 }} out:slide={{ out: true }}>
        <h1>{copy[step].title}</h1>
        <p>{copy[step].text}</p>
      </div>
    {/key}
  </div>

  <footer class="actions">
    {#if blockedHint}<p class="hint">{blockedHint}</p>{/if}
    <button class="go" class:done={status === 'granted'} onclick={primary}>
      {#key label}<span class="lbl" in:fade={{ duration: 220 }} out:fade={{ duration: 120 }}>{label}</span>{/key}
    </button>
    {#if perm && status !== 'granted'}
      <button class="later" onclick={next}>{step === 'install' ? 'Later' : 'Not now'}</button>
    {:else}
      <span class="later ghost" aria-hidden="true"></span>
    {/if}
  </footer>
</div>

<style>
  .welcome {
    position: fixed; inset: 0; z-index: 900; display: flex; flex-direction: column; overflow: hidden; background: var(--desk);
    padding: env(safe-area-inset-top) 0 env(safe-area-inset-bottom); color: var(--text); user-select: none; touch-action: pan-y;
  }
  .bloom { position: absolute; width: 120vw; height: 120vw; left: calc(var(--x) - 60vw); border-radius: 50%; filter: blur(60px); opacity: 0.5; pointer-events: none; transition: left 900ms cubic-bezier(0.22, 1, 0.36, 1); }
  .bloom.warm { top: -50vw; background: radial-gradient(circle, rgba(244, 184, 96, 0.28), transparent 65%); }
  .bloom.lilac { bottom: -60vw; background: radial-gradient(circle, rgba(196, 181, 255, 0.2), transparent 65%); }

  .bar { position: relative; display: flex; align-items: center; justify-content: space-between; height: 56px; padding: 0 20px; }
  .dots { display: flex; gap: 6px; }
  .dots span { width: 18px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.12); transition: background 400ms, width 400ms cubic-bezier(0.22, 1, 0.36, 1); }
  .dots span.on { background: rgba(244, 184, 96, 0.55); }
  .dots span.here { width: 34px; background: var(--accent); }
  .skip { color: var(--muted); font-weight: 600; font-size: 14px; padding: 8px 4px; }

  .stage { position: relative; flex: 1; min-height: 0; display: grid; place-items: center; }
  .pic { grid-area: 1 / 1; display: grid; place-items: center; }

  /* Hello: the lamp, with the circle going round it. */
  .hello { position: relative; width: 260px; height: 260px; display: grid; place-items: center; }
  .gem { width: 116px; height: 116px; filter: drop-shadow(0 14px 34px rgba(244, 184, 96, 0.4)); animation: float 4s ease-in-out infinite; }
  .ring { position: absolute; inset: 0; border-radius: 50%; border: 1px dashed rgba(255, 255, 255, 0.1); animation: spin 18s linear infinite; }
  .ring.r1 { inset: 34px; animation-duration: 12s; animation-direction: reverse; }
  .ring span { position: absolute; width: 14px; height: 14px; margin: -7px; border-radius: 50%; background: var(--c); box-shadow: 0 0 14px var(--c); }
  .ring span:nth-child(1) { left: 50%; top: 0; }
  .ring span:nth-child(2) { left: 100%; top: 50%; }
  .ring span:nth-child(3) { left: 14.6%; top: 85.4%; }
  .r1 span { width: 10px; height: 10px; margin: -5px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes float { 50% { transform: translateY(-8px) rotate(2deg); } }

  /* Mic: a voice going out in rings, and a level that moves. */
  .mic { position: relative; width: 240px; height: 260px; display: grid; place-items: center; --c: var(--accent); }
  .mic.ok { --c: var(--speaking); }
  .disc {
    position: relative; z-index: 2; width: 128px; height: 128px; border-radius: 50%; display: grid; place-items: center; margin-top: -40px;
    color: var(--on-accent); background: radial-gradient(circle at 35% 30%, #ffe0a8, var(--c)); box-shadow: 0 16px 50px color-mix(in srgb, var(--c) 45%, transparent);
    transition: background 500ms, box-shadow 500ms; animation: breathe 2.4s ease-in-out infinite;
  }
  .mic.ok .disc { background: radial-gradient(circle at 35% 30%, #bbf7d0, var(--c)); color: #052e16; }
  .wave { position: absolute; top: calc(50% - 104px); left: calc(50% - 64px); width: 128px; height: 128px; border-radius: 50%; border: 2px solid var(--c); opacity: 0; animation: ripple 2.4s cubic-bezier(0.22, 1, 0.36, 1) infinite; }
  .w2 { animation-delay: 0.8s; }
  .w3 { animation-delay: 1.6s; }
  @keyframes ripple { from { transform: scale(1); opacity: 0.55; } to { transform: scale(1.9); opacity: 0; } }
  @keyframes breathe { 50% { transform: scale(1.04); } }
  .eq { position: absolute; bottom: 8px; display: flex; align-items: center; gap: 5px; height: 40px; }
  .eq i { width: 5px; height: 100%; border-radius: 3px; background: var(--c); transform-origin: center; animation: talk 1.1s calc(var(--i) * -0.13s) ease-in-out infinite alternate; transition: background 500ms; }
  @keyframes talk { 0% { transform: scaleY(0.15); } 60% { transform: scaleY(0.9); } 100% { transform: scaleY(0.35); } }
  .tick {
    position: absolute; z-index: 3; display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; color: #052e16; background: var(--speaking);
    box-shadow: 0 0 0 4px var(--desk), 0 0 20px rgba(74, 222, 128, 0.6); animation: pop 460ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .mic .tick { top: 48px; right: 50px; }
  @keyframes pop { from { transform: scale(0); } }

  /* Notifications: the call card dropping into a phone's shade. */
  .notif { position: relative; width: 270px; height: 260px; display: grid; place-items: center; }
  .phone { width: 250px; height: 200px; border-radius: 28px 28px 10px 10px; padding: 12px 12px 0; background: linear-gradient(#2a2233, #1a1521); border: 1px solid var(--line); overflow: hidden; mask-image: linear-gradient(#000 70%, transparent); }
  .sb { display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; color: var(--muted); padding: 0 8px 12px; }
  .sbi { width: 34px; height: 9px; border-radius: 3px; background: rgba(255, 255, 255, 0.2); }
  .card {
    display: grid; grid-template-columns: auto 1fr; gap: 4px 10px; padding: 11px 12px; border-radius: 16px; background: #2f2739; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
    animation: drop 3.6s cubic-bezier(0.22, 1, 0.36, 1) infinite;
  }
  .ok .card { animation: drop-once 700ms cubic-bezier(0.22, 1, 0.36, 1) both; }
  .nic { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; color: var(--on-accent); background: var(--accent); }
  .nic svg { width: 16px; }
  .nt { display: flex; flex-direction: column; line-height: 1.25; }
  .nt b { font-size: 13px; }
  .nt small { font-size: 11.5px; color: var(--muted); }
  .nb { grid-column: 2; display: flex; gap: 16px; padding-top: 6px; font-size: 12px; font-weight: 700; color: var(--accent); }
  @keyframes drop { 0% { transform: translateY(-120%); opacity: 0; } 14%, 78% { transform: none; opacity: 1; } 92%, 100% { transform: translateY(-120%); opacity: 0; } }
  @keyframes drop-once { from { transform: translateY(-120%); opacity: 0; } }
  .bell {
    position: absolute; right: 2px; bottom: 26px; width: 62px; height: 62px; border-radius: 50%; display: grid; place-items: center; color: var(--on-accent);
    background: radial-gradient(circle at 35% 30%, #ffe0a8, var(--accent)); box-shadow: 0 0 0 5px var(--desk), 0 12px 30px rgba(244, 184, 96, 0.4);
  }
  .bell svg { animation: ring 3.6s ease-in-out infinite; transform-origin: 50% 10%; }
  .ok .bell svg { animation: none; }
  .bell .tick { width: 24px; height: 24px; top: -4px; right: -4px; }
  @keyframes ring { 0%, 10%, 30%, 100% { transform: none; } 14% { transform: rotate(16deg); } 18% { transform: rotate(-14deg); } 22% { transform: rotate(9deg); } 26% { transform: rotate(-5deg); } }

  /* Updates: the diamond fills up like the splash does while it downloads. */
  .inst { position: relative; width: 240px; height: 260px; display: grid; place-items: center; }
  .outline { position: absolute; width: 200px; height: 200px; overflow: visible; }
  .outline path { fill: none; stroke-width: 5; stroke-linejoin: round; }
  .track { stroke: rgba(255, 255, 255, 0.08); }
  .fill { stroke: var(--accent); stroke-dasharray: 100; stroke-dashoffset: 100; filter: drop-shadow(0 0 8px rgba(244, 184, 96, 0.6)); animation: load 2.8s cubic-bezier(0.65, 0, 0.35, 1) infinite; }
  .ok .fill { stroke: var(--speaking); animation: none; stroke-dashoffset: 0; transition: stroke 500ms; filter: drop-shadow(0 0 10px rgba(74, 222, 128, 0.6)); }
  @keyframes load { 0% { stroke-dashoffset: 100; } 70%, 85% { stroke-dashoffset: 0; opacity: 1; } 100% { stroke-dashoffset: 0; opacity: 0; } }
  .arrow { color: var(--accent); animation: nudge 1.4s ease-in-out infinite; }
  .ok .arrow { color: var(--speaking); animation: pop 460ms cubic-bezier(0.34, 1.56, 0.64, 1); }
  @keyframes nudge { 50% { transform: translateY(6px); } }
  .chip { position: absolute; bottom: 0; padding: 5px 12px; border-radius: 999px; font-size: 12px; font-weight: 700; color: var(--accent); background: var(--accent-soft); border: 1px solid rgba(244, 184, 96, 0.25); }

  /* Done: a tick on the lamp, and where each thing stands. */
  .done { display: flex; flex-direction: column; align-items: center; gap: 26px; }
  .gem.small { width: 104px; height: 104px; animation: pop 600ms cubic-bezier(0.34, 1.56, 0.64, 1), float 4s 600ms ease-in-out infinite; }
  .ck { stroke-dasharray: 1; stroke-dashoffset: 1; animation: draw 520ms 380ms cubic-bezier(0.65, 0, 0.35, 1) forwards; }
  @keyframes draw { to { stroke-dashoffset: 0; } }
  ul { list-style: none; margin: 0; padding: 0; width: 260px; display: flex; flex-direction: column; gap: 8px; }
  li {
    display: flex; align-items: center; justify-content: space-between; padding: 11px 14px; border-radius: 14px; background: rgba(255, 255, 255, 0.04); border: 1px solid var(--line);
    font-weight: 600; font-size: 14px; animation: rise 480ms calc(500ms + var(--i) * 90ms) cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  li b { display: inline-flex; align-items: center; gap: 4px; color: var(--speaking); font-size: 12.5px; }
  li button { color: var(--accent); font-size: 12.5px; font-weight: 700; }
  @keyframes rise { from { opacity: 0; transform: translateY(10px); } }

  .copy { display: grid; padding: 0 28px; min-height: 150px; }
  .lines { grid-area: 1 / 1; text-align: center; }
  h1 { margin: 0 0 10px; font-size: 30px; font-weight: 750; font-stretch: 80%; letter-spacing: -0.01em; line-height: 1.05; font-variation-settings: 'opsz' 48; }
  p { margin: 0 auto; max-width: 340px; font-size: 15px; line-height: 1.5; color: var(--muted); }

  .actions { display: flex; flex-direction: column; align-items: stretch; gap: 4px; padding: 8px 24px 18px; }
  .hint { margin: 0 0 8px; padding: 9px 12px; border-radius: 12px; font-size: 13px; line-height: 1.4; color: var(--text); background: rgba(244, 184, 96, 0.1); text-align: center; }
  .go {
    position: relative; display: grid; height: 54px; border-radius: 16px; font-size: 16px; font-weight: 700; color: var(--on-accent);
    background: linear-gradient(135deg, #ffd690, var(--accent)); box-shadow: 0 10px 30px rgba(244, 184, 96, 0.3); transition: transform 140ms, background 400ms, box-shadow 400ms;
  }
  .go:active { transform: scale(0.97); }
  .go.done { background: linear-gradient(135deg, #bbf7d0, var(--speaking)); color: #052e16; box-shadow: 0 10px 30px rgba(74, 222, 128, 0.25); }
  .lbl { grid-area: 1 / 1; place-self: center; }
  .later { height: 44px; color: var(--muted); font-weight: 600; font-size: 14.5px; }
  .later.ghost { display: block; }

  @media (prefers-reduced-motion: reduce) {
    .welcome *, .welcome *::before { animation: none !important; }
    .fill { stroke-dashoffset: 30; }
  }
</style>
