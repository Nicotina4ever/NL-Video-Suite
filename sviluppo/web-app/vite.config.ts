import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The Rust server is the only source of truth; the dev proxy keeps the shell
// honest against a live instance on 8099 (anteprima) or any --port chosen.
const backend = process.env.NL_SUITE_BACKEND || 'http://127.0.0.1:8099';

export default defineConfig({
  plugins: [svelte()],
  // The binary serves the built shell under /shell, so all asset URLs must
  // be prefixed accordingly (dev server exposes /shell/ too).
  base: '/shell/',
  build: { outDir: '../web-dist', emptyOutDir: true },
  // Exposed to the client as __NL_BACKEND__: the shell embeds the classic UI
  // (served by the Rust binary) in an iframe, and in dev the two live on
  // different ports. In production the shell uses a relative URL instead.
  define: { __NL_BACKEND__: JSON.stringify(process.env.NL_SUITE_BACKEND || 'http://127.0.0.1:8099') },
  server: {
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      '/api': { target: backend, changeOrigin: false },
      '/guida': { target: backend, changeOrigin: false },
    },
  },
});
