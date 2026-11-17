import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { api } from './api'
import { isAndroid, isMobile } from './platform'
import { BOOT_WAIT_MS, CHECK_EVERY_MS, checkDue, progressOf, type CheckResult, type UpdatePhase } from './update'

/** A newer Android build, as `apk_update_check` reports it. */
interface ApkUpdate { version: string; url: string; signature: string }

/** The Android bridge's update half (MainActivity.Bridge). */
interface InstallBridge { canInstall(): boolean; allowInstall(): void; installApk(path: string): boolean }
const bridge = () => (window as unknown as { RynuvoAndroid?: Partial<InstallBridge> }).RynuvoAndroid

/**
 * Finds, downloads and applies new versions.
 * - Desktop: the updater plugin; a downloaded update installs and relaunches on its own, even in voice.
 * - Android: the APK from the same `latest.json`, verified in Rust, then the system installer
 *   (Android always asks the user to confirm that step).
 * - iOS updates only through the App Store / TestFlight, so nothing runs there.
 * At launch the splash waits briefly for the first check; if there is an update, the splash
 * becomes the update screen (progress, then restart or install) instead of opening the app.
 */
class Updater {
  state = $state<UpdatePhase>({ phase: 'idle' })
  /** The launch check still owns the splash. */
  boot = $state(true)
  private pending: Update | null = null
  private apk: { version: string; path: string } | null = null
  /** An Android update the user put off ("Later"): not offered again until asked for. */
  private later: string | null = null
  private timer: ReturnType<typeof setInterval> | null = null
  private countdown: ReturnType<typeof setInterval> | null = null
  private last: number | null = null
  private installing = false

  private get supported() {
    return !import.meta.env.DEV && (!isMobile || isAndroid)
  }

  start() {
    if (this.timer) return
    if (!this.supported) {
      this.boot = false
      return
    }
    // The splash doesn't wait forever on a slow network: after a moment the app opens, and an
    // update found later goes the usual in-app way.
    setTimeout(() => this.releaseBoot(), BOOT_WAIT_MS)
    void this.check().then(() => this.releaseBoot())
    this.timer = setInterval(() => void this.check(), CHECK_EVERY_MS)
    window.addEventListener('focus', () => this.nudge())
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState !== 'visible') return
      this.nudge()
      // Back from Android's "install unknown apps" switch: carry on with the install.
      if (this.state.phase === 'install' && this.state.needsPermission && bridge()?.canInstall?.()) void this.install()
    })
  }

  /** Let the splash go unless it is already showing an update. */
  private releaseBoot() {
    if (this.state.phase === 'idle') this.boot = false
  }

  nudge() {
    if (this.timer && checkDue(this.last, Date.now())) void this.check()
  }

  checkNow(): Promise<CheckResult> {
    // Asked for: an Android update put off earlier is offered again straight away.
    if (this.apk && this.later === this.apk.version && this.state.phase === 'idle') {
      this.later = null
      this.state = { phase: 'install', version: this.apk.version }
      return Promise.resolve('found')
    }
    if (this.state.phase !== 'idle') return Promise.resolve('found')
    return new Promise((done) => void this.check(done))
  }

  private async check(answer: (r: CheckResult) => void = () => {}) {
    if (this.state.phase !== 'idle') return answer('found')
    this.last = Date.now()
    try {
      if (isAndroid) await this.checkApk(answer)
      else await this.checkDesktop(answer)
    } catch (e) {
      console.warn('update check failed', e)
      answer('failed')
      this.state = { phase: 'idle' }
      this.boot = false
    }
  }

  private async checkDesktop(answer: (r: CheckResult) => void) {
    const u = await check()
    if (!u) return answer('latest')
    answer('found')
    this.state = { phase: 'downloading', version: u.version, progress: 0 }
    let total: number | null = null
    let got = 0
    await u.download((e) => {
      if (e.event === 'Started') total = e.data.contentLength ?? null
      else if (e.event === 'Progress') {
        got += e.data.chunkLength
        this.state = { phase: 'downloading', version: u.version, progress: progressOf(got, total) }
      }
    })
    this.pending = u
    // Found while the splash was up: nothing to interrupt yet, so straight to the restart.
    if (this.boot) return void this.restart()
    this.state = { phase: 'ready', version: u.version, seconds: 3 }
    this.beginCountdown()
  }

  private async checkApk(answer: (r: CheckResult) => void) {
    const u = await invoke<ApkUpdate | null>('apk_update_check')
    if (!u) return answer('latest')
    answer('found')
    if (this.later === u.version) return
    if (this.apk?.version !== u.version) {
      this.state = { phase: 'downloading', version: u.version, progress: 0 }
      const off = await listen<{ received: number; total: number | null }>('apk-update-progress', (e) => {
        this.state = { phase: 'downloading', version: u.version, progress: progressOf(e.payload.received, e.payload.total) }
      })
      try {
        const path = await invoke<string>('apk_update_download', { url: u.url, signature: u.signature })
        this.apk = { version: u.version, path }
      } finally {
        off()
      }
    }
    this.state = { phase: 'install', version: u.version }
    // From the splash, the system's install prompt comes up right away.
    if (this.boot) void this.install()
  }

  /** Android: hand the APK to the system installer (asking for the permission first if needed). */
  async install() {
    const b = bridge()
    if (!this.apk || !b?.installApk || (this.state.phase !== 'install' && this.state.phase !== 'failed')) return
    if (b.canInstall && !b.canInstall()) {
      this.state = { phase: 'install', version: this.apk.version, needsPermission: true }
      b.allowInstall?.()
      return
    }
    this.state = { phase: 'install', version: this.apk.version }
    // Leave the call first: the installer replaces the app, and that ends it anyway.
    await api.voiceLeave().catch(() => {})
    if (!b.installApk(this.apk.path)) this.state = { phase: 'failed', version: this.apk.version, error: 'The installer could not be opened' }
  }

  /** Android: not now. The app opens; Settings → Check for updates offers it again. */
  postpone() {
    if (this.state.phase !== 'install') return
    this.later = this.state.version
    this.state = { phase: 'idle' }
    this.boot = false
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
    if (isAndroid) return void this.install()
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
      this.boot = false
    } finally {
      this.installing = false
    }
  }
}

export const updater = new Updater()
