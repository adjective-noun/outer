import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
	plugins: [sveltekit()],
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'node',
		coverage: {
			provider: 'v8',
			reporter: ['text', 'json', 'html'],
			exclude: [
				'node_modules/**',
				'.svelte-kit/**',
				'coverage/**',
				'**/*.config.{js,ts}',
				'**/*.d.ts'
			],
			thresholds: {
				statements: 0,
				branches: 0,
				functions: 0,
				lines: 0
			}
		}
	}
});
