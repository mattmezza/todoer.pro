import { copyFile } from 'node:fs/promises';

// The ESM distribution includes its reactivity dependencies and never auto-starts.
// Register Todoer components before calling Alpine.start() in alpine.js.
await copyFile(
  'node_modules/@alpinejs/csp/dist/module.esm.min.js',
  'app/assets/alpine-csp.min.js',
);
