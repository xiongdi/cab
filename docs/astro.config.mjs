// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

/** @type {import('astro').AstroUserConfig} */
export default defineConfig({
	site: 'https://xiongdi.github.io',
	base: '/cab/',
	integrations: [
		starlight({
			title: 'CAB',
			description:
				'CAB (Coding Agents Bridge) — local LLM gateway and smart router for coding agents.',
			defaultLocale: 'root',
			locales: {
				root: {
					label: 'English',
					lang: 'en',
				},
				'zh-cn': {
					label: '简体中文',
					lang: 'zh-CN',
				},
			},
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/xiongdi/cab',
				},
			],
			editLink: {
				baseUrl: 'https://github.com/xiongdi/cab/edit/main/docs/',
			},
			sidebar: [
				{
					label: 'Getting Started',
					translations: { 'zh-CN': '快速开始' },
					items: [
						{ slug: 'getting-started/install' },
						{ slug: 'getting-started/quick-start' },
					],
				},
				{
					label: 'Guides',
					translations: { 'zh-CN': '使用指南' },
					items: [
						{ slug: 'guides/providers-and-models' },
						{ slug: 'guides/routing' },
						{ slug: 'guides/agents' },
						{ slug: 'guides/gateway-auth' },
					],
				},
				{
					label: 'Reference',
					translations: { 'zh-CN': '参考' },
					items: [
						{ slug: 'reference/supported-agents' },
						{ slug: 'reference/architecture' },
						{ slug: 'reference/api' },
					],
				},
				{
					label: 'Project',
					translations: { 'zh-CN': '项目' },
					items: [{ slug: 'project/changelog' }],
				},
			],
		}),
	],
});
