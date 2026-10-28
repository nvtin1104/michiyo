import { defineConfig } from 'vite';
import { fileURLToPath, URL } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: [
      {
        find: '@michiyo/shared/ids',
        replacement: fileURLToPath(
          new URL('./shared/src/ids.ts', import.meta.url),
        ),
      },
      {
        find: '@michiyo/shared',
        replacement: fileURLToPath(
          new URL('./shared/src/index.ts', import.meta.url),
        ),
      },
    ],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Cargo creates and locks executables here; Tauri watches Rust sources itself.
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
});
