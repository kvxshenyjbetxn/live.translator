import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  test: { environment: 'jsdom', include: ['tests/**/*.test.ts'], setupFiles: ['tests/setup.ts'] },
});
