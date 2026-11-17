import { convertFileSrc } from '@tauri-apps/api/core'
import { api } from './api'
import { videoUrls } from './video'

/**
 * The app's one list of music video links. Asking for the YouTube link first checks the video can
 * be played (and warms the backend's cache); the player then streams it through `rynuvo-video://`,
 * which fetches in bounded chunks that YouTube doesn't throttle.
 */
export const videos = videoUrls((id) => api.musicVideoUrl(id).then(() => convertFileSrc(id, 'rynuvo-video')))
