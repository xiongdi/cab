import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite-plus';

export default defineConfig({
  fmt: {
    singleQuote: true,
    trailingComma: 'es5',
    printWidth: 100,
    sortPackageJson: false,
    ignorePatterns: [
      'node_modules',
      '.svelte-kit',
      'build',
      'target',
      'spec/dist',
      'spec/node_modules',
      'spec/.astro',
      'package-lock.json',
      'Cargo.lock',
    ],
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
  plugins: [sveltekit()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**', '**/target/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari15',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
