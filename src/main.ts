import './app.css';
// xterm.js renders inside a sized container with its own canvas + DOM
// helpers. Without its base stylesheet the cell measurements collapse
// and the Terminal tab shows a 1-row strip with no input. Import it
// once at the app entry so every Terminal instance gets it.
import '@xterm/xterm/css/xterm.css';
import App from './App.svelte';
import { mount } from 'svelte';

const app = mount(App, { target: document.getElementById('app')! });

export default app;
