/** Phones show one pane at a time; App owns it, and the pane's own header carries the way back. */
export const pane = $state({ narrow: false, close: () => {} })
