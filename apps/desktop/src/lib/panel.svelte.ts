/** Whether the music panel shows beside the voice room (this app only, remembered). When hidden, a
 *  mini player sits in the room's header instead. */
export const musicPanel = (() => {
  let open = $state(read())
  function read() {
    try {
      return localStorage.getItem('music.panel') !== '0'
    } catch {
      return true
    }
  }
  return {
    get open() { return open },
    set open(v: boolean) {
      open = v
      try {
        localStorage.setItem('music.panel', v ? '1' : '0')
      } catch {
        // not remembered; fine
      }
    },
  }
})()
