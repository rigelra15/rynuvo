import { mount } from 'svelte'
import './theme.css'
import App from './App.svelte'
import Overlay from './overlay/Overlay.svelte'
import { isIOS } from './lib/platform'
import { installLongPress } from './lib/longpress'

if (isIOS) installLongPress()

// The call overlay is this same page in its own window.
const label = (window as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } }).__TAURI_INTERNALS__?.metadata?.currentWindow?.label
const app = mount(label === 'overlay' ? Overlay : App, { target: document.getElementById('app')! })

export default app
