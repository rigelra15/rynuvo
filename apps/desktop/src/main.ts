import { mount } from 'svelte'
import './theme.css'
import App from './App.svelte'
import { isIOS } from './lib/platform'
import { installLongPress } from './lib/longpress'

if (isIOS) installLongPress()

const app = mount(App, { target: document.getElementById('app')! })

export default app
