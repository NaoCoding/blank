import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    strictPort: true,
    watch: { ignored: ['**/src-tauri/target/**'] }
  },
  optimizeDeps: {
    noDiscovery: true,
    include: [
      '@xterm/xterm', 'markdown-it', 'markdown-it-task-lists',
      'highlight.js/lib/core',
      'highlight.js/lib/languages/javascript', 'highlight.js/lib/languages/typescript',
      'highlight.js/lib/languages/python', 'highlight.js/lib/languages/rust',
      'highlight.js/lib/languages/json', 'highlight.js/lib/languages/css',
      'highlight.js/lib/languages/xml'
    ],
    exclude: ['svelte']
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: { target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13' },
  test: { environment: 'jsdom' }
});
