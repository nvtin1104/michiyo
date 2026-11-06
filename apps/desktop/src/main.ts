import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';
import { desktop } from './lib/api';
document.documentElement.dataset.desktop = String(desktop);
document.documentElement.dataset.view =
  new URLSearchParams(location.search).get('view') || 'chat';
mount(App, { target: document.getElementById('app')! });
