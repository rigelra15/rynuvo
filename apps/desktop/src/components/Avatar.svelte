<script lang="ts">
  import { avatarColor, isAvatarIcon } from '../lib/avatar'
  import { AVATAR_SHAPES } from '../lib/avatarIcons'
  import { mediaUrl } from '../lib/api'
  import type { User } from '../lib/types'

  let { user, id = 0, size = 32, speaking = false }: { user?: User | null; id?: number; size?: number; speaking?: boolean } = $props()

  const color = $derived(avatarColor(user ?? { id }))
  const icon = $derived(isAvatarIcon(user?.avatarIcon) ? user!.avatarIcon! : null)
  const letter = $derived((user?.nickname ?? '?').slice(0, 1).toUpperCase())
  // A photo that fails to load falls back to the icon or letter.
  let broken = $state<string | null>(null)
  const photo = $derived(user?.avatarImage && user.avatarImage !== broken ? user.avatarImage : null)
</script>

<span class="avatar" class:speaking style="--c:{color};--s:{size}px">
  {#if speaking}<span class="ring"></span><span class="ring r2"></span>{/if}
  <span class="face">
    {#if photo}
      <img class="pic" src={mediaUrl(photo)} alt="" draggable="false" onerror={() => (broken = photo)} />
    {:else if icon}
      <svg width={size * 0.56} height={size * 0.56} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {#each AVATAR_SHAPES[icon as keyof typeof AVATAR_SHAPES] as s, i (i)}
          {#if 'd' in s}<path d={s.d} />{:else if 'c' in s}<circle cx={s.c[0]} cy={s.c[1]} r={s.c[2]} />{:else}<ellipse cx={s.e[0]} cy={s.e[1]} rx={s.e[2]} ry={s.e[3]} />{/if}
        {/each}
      </svg>
    {:else}
      {letter}
    {/if}
  </span>
</span>

<style>
  .avatar { position: relative; width: var(--s); height: var(--s); flex-shrink: 0; display: inline-grid; }
  .face {
    position: relative; z-index: 1; width: 100%; height: 100%; border-radius: 50%;
    background: var(--c); color: #fff; display: grid; place-items: center;
    font-weight: 700; font-size: calc(var(--s) * 0.42); line-height: 1;
  }
  .pic { width: 100%; height: 100%; border-radius: 50%; object-fit: cover; }
  /* Speaking: rings in the user's colour grow out of the avatar and fade. */
  .ring {
    position: absolute; inset: 0; border-radius: 50%; border: 2px solid var(--c);
    animation: wave 1.3s ease-out infinite; pointer-events: none;
  }
  .ring.r2 { animation-delay: 0.65s; }
  @keyframes wave {
    from { transform: scale(1); opacity: 0.85; }
    to { transform: scale(1.75); opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring { animation: none; transform: scale(1.15); opacity: 0.8; }
    .ring.r2 { display: none; }
  }
</style>
