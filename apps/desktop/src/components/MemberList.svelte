<script lang="ts">
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'
  import { memberGroups } from '../lib/members'
  import type { Id, User } from '../lib/types'

  let { onMenu }: { onMenu: (userId: Id, e: MouseEvent) => void } = $props()

  const groups = $derived(memberGroups(store.users.values(), store.online, store.me?.id))

  /** The voice channel a member is in, if any. */
  function inVoice(id: Id): string | null {
    const v = store.voice.find((m) => m.userId === id)
    return v ? (store.channels.find((c) => c.id === v.channelId)?.name ?? null) : null
  }
</script>

{#snippet row(u: User, on: boolean)}
  {@const voice = on ? inVoice(u.id) : null}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="member" class:off={!on} oncontextmenu={(e) => { e.preventDefault(); onMenu(u.id, e) }}>
    <span class="av">
      <Avatar user={u} id={u.id} size={32} speaking={store.speaking.has(u.id)} />
      <span class="dot" class:on></span>
    </span>
    <span class="text">
      <span class="nick">{u.nickname}{#if u.id === store.me?.id}<span class="you"> (you)</span>{/if}</span>
      {#if voice}<span class="sub"><Icon name="volume" size={12} />{voice}</span>{:else if u.isAdmin}<span class="sub">Admin</span>{/if}
    </span>
  </div>
{/snippet}

<aside class="members" aria-label="Members">
  <h4>Online — {groups.online.length}</h4>
  {#each groups.online as u (u.id)}{@render row(u, true)}{/each}
  {#if groups.offline.length}
    <h4>Offline — {groups.offline.length}</h4>
    {#each groups.offline as u (u.id)}{@render row(u, false)}{/each}
  {/if}
</aside>

<style>
  .members { width: 220px; flex-shrink: 0; overflow-y: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 12px 8px; }
  h4 { margin: 12px 8px 6px; font-size: 11.5px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); }
  h4:first-child { margin-top: 4px; }
  .member { display: flex; align-items: center; gap: 10px; padding: 5px 8px; border-radius: 6px; cursor: default; }
  .member:hover { background: var(--elev-2); }
  .member.off { opacity: 0.45; }
  .member.off:hover { opacity: 0.8; }
  .av { position: relative; display: inline-flex; }
  .dot { position: absolute; right: -2px; bottom: -2px; z-index: 2; width: 12px; height: 12px; border-radius: 50%; background: var(--faint); border: 3px solid var(--panel); box-sizing: border-box; }
  .dot.on { background: var(--speaking); }
  .member:hover .dot { border-color: var(--elev-2); }
  .text { display: flex; flex-direction: column; min-width: 0; }
  .nick { font-size: 14px; font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .you { color: var(--muted); font-weight: 400; }
  .sub { display: flex; align-items: center; gap: 4px; font-size: 11.5px; color: var(--muted); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
</style>
