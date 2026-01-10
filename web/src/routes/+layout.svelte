<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { initializeConnection, connected, error } from '$lib/stores';

	let initialized = false;

	onMount(() => {
		initializeConnection()
			.then(() => {
				initialized = true;
			})
			.catch((e) => {
				console.error('Failed to connect:', e);
			});
	});
</script>

<div class="app">
	{#if $error}
		<div class="error-toast">{$error}</div>
	{/if}

	{#if !$connected && initialized}
		<div class="connection-status">
			<div class="spinner"></div>
			Reconnecting...
		</div>
	{/if}

	<slot />
</div>

<style>
	.app {
		height: 100%;
		display: flex;
		flex-direction: column;
	}

	.error-toast {
		position: fixed;
		top: 16px;
		left: 50%;
		transform: translateX(-50%);
		background: var(--color-error);
		color: white;
		padding: 12px 24px;
		border-radius: var(--radius-md);
		z-index: 1000;
		animation: slideIn 0.3s ease-out;
	}

	.connection-status {
		position: fixed;
		bottom: 16px;
		left: 50%;
		transform: translateX(-50%);
		background: var(--color-bg-tertiary);
		color: var(--color-text-muted);
		padding: 8px 16px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		gap: 8px;
		z-index: 1000;
	}

	.spinner {
		width: 16px;
		height: 16px;
		border: 2px solid var(--color-border);
		border-top-color: var(--color-primary);
		border-radius: 50%;
		animation: spin 1s linear infinite;
	}

	@keyframes slideIn {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(-20px);
		}
		to {
			opacity: 1;
			transform: translateX(-50%) translateY(0);
		}
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
