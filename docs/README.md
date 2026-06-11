# CAB Documentation Site

Official documentation for [CAB (Coding Agents Bridge)](https://github.com/xiongdi/cab), built with [Astro](https://astro.build/) and [Starlight](https://starlight.astro.build/).

Published at: **https://xiongdi.github.io/cab/**

## Local development

```bash
cd docs
npm install
npm run dev
```

The dev server starts at `http://localhost:4321/cab/` (base path is configured for GitHub Pages).

## Build & preview

```bash
npm run build
npm run preview
```

## Content structure

| Path | Description |
| ---- | ----------- |
| `src/content/docs/` | English pages (default locale) |
| `src/content/docs/zh-cn/` | Simplified Chinese pages |
| `astro.config.mjs` | Site, base path, sidebar, and i18n config |

Use **relative links** in Markdown and hero actions (for example `install/`), not root-absolute paths like `/install/`. With `base: '/cab/'`, root-absolute links skip the project prefix on GitHub Pages.

## Deployment

Pushes to `main` that touch `docs/**` trigger the [Deploy Docs](../../.github/workflows/docs.yml) workflow, which publishes the built site to GitHub Pages.

Ensure the repository **Settings → Pages → Build and deployment** source is set to **GitHub Actions**.
