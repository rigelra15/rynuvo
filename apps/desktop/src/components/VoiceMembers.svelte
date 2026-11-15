<script lang="ts">
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import type { Id } from '../lib/types'
  import Icon from './Icon.svelte'

  let { channelId, onMenu }: { channelId: Id; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channelId))
  const music = $derived(store.musicIn(channelId))
</script>

{#if music}
  <p class="np" title="{music.state.now!.title} — {music.state.now!.artist}"><Icon name="music" size={12} /> {music.state.now!.title} — {music.state.now!.artist}</p>
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
  .np { margin: 0 0 2px; padding: 0 8px 0 34px; display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--accent); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .vm { list-style: none; margin: 0 0 4px; padding: 0 0 0 28px; }
  li { display: flex; align-items: center; gap: 7px; padding: 3px 6px; border-radius: 6px; color: var(--muted); font-size: 13px; }
  li:hover { background: var(--elev-2); color: var(--text); }
  .n { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st { color: var(--faint); display: grid; }
</style>
