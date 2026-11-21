/// <reference types="svelte" />
/// <reference types="vite/client" />

// Injected by vite.config.ts: the Rust backend origin, so the shell can embed
// the classic UI in dev (where vite owns port 5173) and use a relative URL in
// production (where the binary serves both / and /shell).
declare const __NL_BACKEND__: string;
