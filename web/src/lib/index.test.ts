import { describe, it, expect } from 'vitest';

describe('library exports', () => {
	it('should export types module', async () => {
		const lib = await import('./index');
		expect(lib).toBeDefined();
	});
});
