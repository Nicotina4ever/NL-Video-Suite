import { mount } from 'svelte';
import App from './App.svelte';
import './tokens.css';

// The suite's default persona is the dark editor theme (Premiere-style);
// a stored preference or an explicit light system theme wins, same rule the
// current UI already applies in appearance.js.
const stored = localStorage.getItem('nl-theme');
const theme =
  stored === 'light' || stored === 'dark'
    ? stored
    : window.matchMedia?.('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark';
document.documentElement.dataset.theme = theme;

const app = mount(App, { target: document.getElementById('app')! });
export default app;
