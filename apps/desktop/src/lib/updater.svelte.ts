import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { CHECK_EVERY_MS, checkDue, type CheckResult, type UpdatePhase } from './update'

class Updater {
  state = $state<UpdatePhase>({ phase: 'idle' })
  private pending: Update | null = null
  private timer: ReturnType<typeof setInterval> | null = null
  private last: number | null = null

  start() {
    if (this.timer || import.meta.env.DEV) return
    void this.check()
    this.timer = setInterval(() => void this.check(), CHECK_EVERY_MS)
    // Coming back to the app is a good moment to look: the download then runs while you use it.
    window.addEventListener('focus', () => this.nudge())
    document.addEventListener('visibilitychange', () => document.visibilityState === 'visible' && this.nudge())
  }

  /** Look now unless we just did (focus, back online). */
  nudge() {
    if (this.timer && checkDue(this.last, Date.now())) void this.check()
  }

  /** Asked for from Settings: answers once it knows whether there is a new version (the download
   *  then carries on in the background). */
  checkNow(): Promise<CheckResult> {
    if (this.state.phase !== 'idle') return Promise.resolve('found')
    return new Promise((done) => void this.check(done))
  }

  private async check(answer: (r: CheckResult) => void = () => {}) {
    if (this.state.phase !== 'idle') return answer('found')
    this.last = Date.now()
    try {
      const u = await check()
      if (!u) return answer('latest')
      answer('found')
      this.state = { phase: 'downloading', version: u.version }
      await u.download()
      this.pending = u
      this.state = { phase: 'ready', version: u.version }
    } catch (e) {
      console.warn('update check failed', e) // offline or server down: try again later
      answer('failed')
      this.state = { phase: 'idle' }
    }
  }

  async restart() {
    if (!this.pending) return
    await this.pending.install()
    await relaunch()
  }
}

export const updater = new Updater()
