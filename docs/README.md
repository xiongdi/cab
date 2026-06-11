# CAB Official Site

Official website and documentation for [CAB (Coding Agents Bridge)](https://github.com/xiongdi/cab), built with [Astro](https://astro.build/) + [Starlight](https://starlight.astro.build/).

Published at: **https://xiongdi.github.io/cab/**

## Site structure

| Section | Purpose |
| ------- | ------- |
| **Home** | Product landing — value prop, features, CTAs |
| **Getting Started** | Install + 5-minute quick start |
| **Guides** | Providers, routing, agents, gateway auth |
| **Reference** | Supported agents, architecture, API |
| **Project** | Changelog |

Each section has English (`src/content/docs/`) and Simplified Chinese (`src/content/docs/zh-cn/`) pages.

## Local development

```bash
cd docs
npm install
npm run dev    # http://localhost:4321/cab/
npm run build
```

Mermaid diagrams are rendered **client-side** via `astro-mermaid` (supports light/dark theme switching).

## Link conventions

Use **relative links** in Markdown and hero actions (e.g. `getting-started/install/`), not root-absolute paths like `/install/`. With `base: '/cab/'`, root-absolute links break on GitHub Pages.

## Deployment

Pushes to `main` that touch `docs/**` trigger [.github/workflows/docs.yml](../.github/workflows/docs.yml).
