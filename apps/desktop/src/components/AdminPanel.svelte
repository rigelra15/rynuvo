<script lang="ts">
  import { onMount } from 'svelte'
  import { api, errText, type Report, type TransferCode } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'
  import Countdown from './Countdown.svelte'
  import { dialog } from '../lib/dialog.svelte'

  let invite = $state<string | null>(null)
  let copied = $state(false)
  let codes = $state<Record<Id, TransferCode>>({})
  let reports = $state<Report[]>([])
  let openReport = $state<Id | null>(null)
  let error = $state<string | null>(null)

  const members = $derived(store.people().sort((a, b) => a.nickname.localeCompare(b.nickname)))

  onMount(() => void loadReports())

  async function loadReports() {
    try {
      reports = await api.adminReports()
    } catch (e) {
      error = errText(e)
    }
  }

  async function newInvite() {
    if (invite === null && !(await dialog.ask('Create a new invite code? The old one stops working.', 'Create', false))) return
    try {
      invite = (await api.adminRotateInvite()).code
      copied = false
    } catch (e) {
      error = errText(e)
    }
  }

  async function copy() {
    if (!invite) return
    await navigator.clipboard.writeText(invite)
    copied = true
  }

  async function kick(id: Id, nick: string) {
    if (!(await dialog.ask(`Remove ${nick} from the circle? Their devices are signed out right away.`, 'Remove'))) return
    try {
      await api.adminKick(id)
    } catch (e) {
      error = errText(e)
    }
  }

  async function transferFor(id: Id) {
    try {
      codes[id] = await api.adminTransferCode(id)
    } catch (e) {
      error = errText(e)
    }
  }

  let full = $state<Record<Id, string>>({})
  async function loadFull(id: Id) {
    try {
      full[id] = await api.adminReportBody(id)
    } catch (e) {
      error = errText(e)
    }
  }

  function summary(r: Report): string {
    if (r.kind !== 'nettest') return `${Math.max(1, Math.round(r.size / 1024))} KB of log`
    try {
      const n = JSON.parse(r.body)
      return `${n.mapping}, UDP ${n.udpOk ? 'ok' : 'blocked'}, IPv6 ${n.ipv6Ok ? 'yes' : 'no'}, ${n.uploadMbps ?? '?'} Mbps`
    } catch {
      return 'unreadable'
    }
  }

  const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
</script>

{#if store.alerts.length}
  <section>
    <h3>Server alerts</h3>
    {#each store.alerts as a}<p class="alert">{a}</p>{/each}
  </section>
{/if}

<section>
  <h3>Invite code</h3>
  <p class="muted">Share it with a friend so they can join. Creating a new one disables the old one.</p>
  {#if invite}
    <div class="row"><span class="code selectable">{invite}</span><button class="btn" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button></div>
    <p class="muted">Shown only once.</p>
  {/if}
  <button class="btn" onclick={newInvite}>Create new invite code</button>
</section>

<section>
  <h3>Members</h3>
  {#each members as u (u.id)}
    <div class="member">
      <span class="dot" class:on={store.online.has(u.id)}></span>
      <span class="nick">{u.nickname}{#if u.isAdmin}<span class="tag">admin</span>{/if}</span>
      {#if codes[u.id]}<span class="tcode selectable">{codes[u.id].code} · <Countdown until={codes[u.id].expiresAt} /></span>{/if}
      {#if u.id !== store.me?.id}
        <button class="btn sm" onclick={() => transferFor(u.id)}>Transfer code</button>
        <button class="btn sm danger" onclick={() => kick(u.id, u.nickname)}>Remove</button>
      {/if}
    </div>
  {/each}
</section>

<section>
  <div class="row between"><h3>Reports</h3><button class="btn sm" onclick={loadReports}>Refresh</button></div>
  {#if !reports.length}<p class="muted">No reports yet.</p>{/if}
  {#each reports as r (r.id)}
    <div class="report">
      <button class="rhead" onclick={() => (openReport = openReport === r.id ? null : r.id)}>
        <span class="kind">{r.kind}</span><span>{r.nickname}</span><span class="muted">{when(r.createdAt)}</span><span class="muted sum">{summary(r)}</span>
      </button>
      {#if openReport === r.id}
        <pre class="selectable">{full[r.id] ?? r.body}</pre>
        {#if r.size > r.body.length && full[r.id] === undefined}
          <button class="btn sm" onclick={() => loadFull(r.id)}>Load full log ({Math.round(r.size / 1024)} KB)</button>
        {/if}
      {/if}
    </div>
  {/each}
</section>

{#if error}<p class="err">{error}</p>{/if}

<style>
  section { padding: 14px 0; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .member, .report, .between { align-self: stretch; }
  h3 { margin: 0; font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  p { margin: 0; font-size: 13px; }
  .row { display: flex; gap: 10px; align-items: center; }
  .between { justify-content: space-between; }
  .code { font-family: var(--mono); font-size: 22px; letter-spacing: 0.08em; color: #fff; }
  .alert { color: #fca5a5; }
  .member { display: flex; align-items: center; gap: 8px; font-size: 14px; }
  .nick { flex: 1; }
  .tag { margin-left: 6px; font-size: 10px; padding: 1px 6px; border-radius: 999px; background: var(--elev); color: var(--accent); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--faint); }
  .dot.on { background: #4ade80; }
  .tcode { font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .btn.sm { padding: 4px 10px; font-size: 12px; }
  .report { border-radius: 8px; background: var(--elev-2); }
  .rhead { width: 100%; display: flex; gap: 10px; padding: 8px 10px; text-align: left; font-size: 13px; }
  .kind { font-family: var(--mono); color: var(--accent); }
  .sum { margin-left: auto; }
  pre { margin: 0; padding: 10px; max-height: 260px; overflow: auto; font-size: 11px; white-space: pre-wrap; }
  .err { color: var(--danger); }
</style>
