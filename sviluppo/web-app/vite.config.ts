import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The Rust server is the only source of truth; the dev proxy keeps the shell
// honest against a live instance on 8099 (anteprima) or any --port chosen.
const backend = process.env.NL_SUITE_BACKEND || 'http://127.0.0.1:8099';

export default defineConfig({
  plugins: [svelte()],
  build: { outDir: '../web-dist', emptyOutDir: true },
  server: {
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      '/api': { target: backend, changeOrigin: false },
      '/guida': { target: backend, changeOrigin: false },
    },
  },
});
