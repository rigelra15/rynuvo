import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { CHECK_EVERY_MS, type UpdatePhase } from './update'

class Updater {
  state = $state<UpdatePhase>({ phase: 'idle' })
  private pending: Update | null = null
  private timer: ReturnType<typeof setInterval> | null = null

  start() {
    if (this.timer || import.meta.env.DEV) return
    void this.check()
    this.timer = setInterval(() => void this.check(), CHECK_EVERY_MS)
  }

  private async check() {
    if (this.state.phase !== 'idle') return
    try {
      const u = await check()
      if (!u) return
      this.state = { phase: 'downloading', version: u.version }
      await u.download()
      this.pending = u
      this.state = { phase: 'ready', version: u.version }
    } catch (e) {
      console.warn('update check failed', e) // offline or server down: try again later
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
