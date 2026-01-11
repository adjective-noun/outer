import js from '@eslint/js';
import ts from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import prettier from 'eslint-config-prettier';

const browserGlobals = {
	window: 'readonly',
	document: 'readonly',
	console: 'readonly',
	localStorage: 'readonly',
	sessionStorage: 'readonly',
	WebSocket: 'readonly',
	setTimeout: 'readonly',
	clearTimeout: 'readonly',
	setInterval: 'readonly',
	clearInterval: 'readonly',
	requestAnimationFrame: 'readonly',
	cancelAnimationFrame: 'readonly',
	fetch: 'readonly',
	alert: 'readonly',
	confirm: 'readonly',
	location: 'readonly',
	history: 'readonly',
	navigator: 'readonly',
	HTMLElement: 'readonly',
	HTMLDivElement: 'readonly',
	SVGSVGElement: 'readonly',
	CustomEvent: 'readonly',
	Event: 'readonly',
	MouseEvent: 'readonly',
	KeyboardEvent: 'readonly'
};

export default [
	js.configs.recommended,
	prettier,
	{
		files: ['**/*.ts'],
		languageOptions: {
			parser: tsParser,
			parserOptions: {
				ecmaVersion: 'latest',
				sourceType: 'module'
			},
			globals: browserGlobals
		},
		plugins: {
			'@typescript-eslint': ts
		},
		rules: {
			...ts.configs.recommended.rules,
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_' }
			],
			'@typescript-eslint/no-explicit-any': 'warn',
			'no-unused-vars': 'off'
		}
	},
	{
		files: ['**/*.svelte'],
		languageOptions: {
			parser: svelteParser,
			parserOptions: {
				parser: tsParser
			},
			globals: browserGlobals
		},
		plugins: {
			svelte,
			'@typescript-eslint': ts
		},
		rules: {
			...svelte.configs.recommended.rules,
			'no-unused-vars': 'off',
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_' }
			]
		}
	},
	{
		ignores: ['.svelte-kit/**', 'build/**', 'node_modules/**', 'coverage/**']
	}
];
