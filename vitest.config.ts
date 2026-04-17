import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    alias: { $lib: resolve(__dirname, 'src/lib') },
    conditions: ['browser'],
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.{test,spec}.{ts,js}'],
    setupFiles: ['./src/test-setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'json'],
      thresholds: { lines: 95, branches: 95, functions: 95, statements: 95 },
      include: ['src/**/*.{ts,svelte}'],
      exclude: [
        'src/main.ts',
        'src/app.d.ts',
        'src/App.svelte',
        'src/lib/types.ts',
        'src/**/*.{test,spec}.ts',
        'src/**/*.d.ts',
        'src/**/__mocks__/**',
      ],
    },
  },
});
