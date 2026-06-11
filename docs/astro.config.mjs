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
			description: 'Official documentation for CAB (Coding Agents Bridge).',
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
					items: [{ slug: 'install' }],
				},
				{
					label: 'Reference',
					translations: { 'zh-CN': '参考' },
					items: [{ slug: 'agents/supported-agents' }],
				},
			],
		}),
	],
});
