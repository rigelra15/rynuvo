/** Running on Android or iOS/iPadOS (iPadOS reports itself as a Mac, but with a touch screen). */
export const isMobile =
  /Android|iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.userAgent.includes('Mac') && navigator.maxTouchPoints > 1)

/** iOS/iPadOS: its WebView never turns a long press into a `contextmenu` event (Android's does). */
export const isIOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.userAgent.includes('Mac') && navigator.maxTouchPoints > 1)

/** Android: updates come as an APK the app downloads itself (iOS only updates through the store). */
export const isAndroid = /Android/.test(navigator.userAgent)
