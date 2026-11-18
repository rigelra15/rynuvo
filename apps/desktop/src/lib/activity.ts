// Desktop: what I'm playing, for others to see ("Playing Brawlhalla"). The app watches for games
// (see game.rs) and this passes the name on, unless sharing it is turned off in Settings.
import { listen } from '@tauri-apps/api/event'
import { api } from './api'

const EVENT = 'rynuvo://activity'
const PREF = 'rynuvo.shareGame'

export function shareGame(): boolean {
  try {
    return localStorage.getItem(PREF) !== '0'
  } catch {
    return true
  }
}

let game: string | null = null
const sync = () => void api.setActivity(shareGame() ? game : null).catch(() => {})

export function setShareGame(on: boolean) {
  try {
    localStorage.setItem(PREF, on ? '1' : '0')
  } catch {
    // Private storage off: it holds for this run only.
  }
  sync()
}

/** Starts passing the game on. */
export function watchActivity() {
  void listen<string | null>(EVENT, ({ payload }) => {
    game = payload
    sync()
  })
}
