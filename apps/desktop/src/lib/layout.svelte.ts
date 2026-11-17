import { isMobile } from './platform'

/** A tablet (touch, but wide enough for the sidebar beside the page). */
export const isTablet = isMobile && Math.min(screen.width, screen.height) >= 600

/** Whether the sidebar is tucked away while a room is open (tablets only). */
export const layout = $state({ sideHidden: false })
