import { api } from './api'
import { videoUrls } from './video'

/** The app's one list of music video links. */
export const videos = videoUrls((id) => api.musicVideoUrl(id))
