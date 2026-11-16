import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { api } from './api'
import { CHECK_EVERY_MS, checkDue, type CheckResult, type UpdatePhase } from './update'

/** A downloaded, verified update must be installed even while a user is in voice. */
class Updater {
  state = $state<UpdatePhase>({ phase: 'idle' })
  private pending: Update | null = null
  private timer: ReturnType<typeof setInterval> | null = null
  private countdown: ReturnType<typeof setInterval> | null = null
  private last: number | null = null
  private installing = false

  start() {
    if (this.timer || import.meta.env.DEV) return
    void this.check()
    this.timer = setInterval(() => void this.check(), CHECK_EVERY_MS)
    window.addEventListener('focus', () => this.nudge())
    document.addEventListener('visibilitychange', () => document.visibilityState === 'visible' && this.nudge())
  }

  nudge() {
    if (this.timer && checkDue(this.last, Date.now())) void this.check()
  }

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
      this.state = { phase: 'ready', version: u.version, seconds: 3 }
      this.beginCountdown()
    } catch (e) {
      console.warn('update check failed', e)
      answer('failed')
      this.state = { phase: 'idle' }
    }
  }

  private beginCountdown() {
    if (this.countdown) return
    this.countdown = setInterval(() => {
      if (this.state.phase !== 'ready') return
      if (this.state.seconds > 1) {
        this.state = { ...this.state, seconds: this.state.seconds - 1 }
      } else {
        if (this.countdown) clearInterval(this.countdown)
        this.countdown = null
        void this.restart()
      }
    }, 1000)
  }

  async restart() {
    if (!this.pending || this.installing) return
    this.installing = true
    const version = this.pending.version
    this.state = { phase: 'installing', version }
    try {
      // Stop the screen capture and close the mic before installing/relaunching.
      // Both commands work whether or not a stream/voice channel is active.
      await api.streamStop()
      await api.voiceLeave()
      await this.pending.install()
      await relaunch()
    } catch (e) {
      console.error('automatic update failed', e)
      // Keep the modal locked, but give a visible retry action instead of a silent dead end.
      this.state = { phase: 'failed', version, error: String(e) }
    } finally {
      this.installing = false
    }
  }
}

export const updater = new Updater()
