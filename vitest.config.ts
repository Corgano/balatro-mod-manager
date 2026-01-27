import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    environment: 'happy-dom',
    globals: true,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      include: ['src/**/*.{js,ts,svelte}'],
      exclude: ['src/**/*.{test,spec}.{js,ts}', 'src/test/**/*'],
    },
    alias: {
      $lib: './src/lib',
    },
  },
});
