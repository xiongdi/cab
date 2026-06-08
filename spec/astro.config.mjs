import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://cab.local/spec',
  srcDir: 'src',
  server: {
    port: 1234
  },
  markdown: {
    shikiConfig: {
      theme: 'github-dark'
    }
  }
});
