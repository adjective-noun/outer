<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { initializeConnection, connected, error } from '$lib/stores';

	let initialized = false;
	let connectionFailed = false;

	onMount(() => {
		initializeConnection()
			.then(() => {
				initialized = true;
				connectionFailed = false;
			})
			.catch((e) => {
				console.error('Failed to connect:', e);
				initialized = true;
				connectionFailed = true;
			});
	});
</script>

<div class="app">
	{#if $error}
		<div class="error-toast">{$error}</div>
	{/if}

	{#if !initialized}
		<!-- Initial connection attempt -->
		<div class="connection-overlay">
			<div class="connection-card">
				<div class="spinner-large"></div>
				<p>Connecting to server...</p>
			</div>
		</div>
	{:else if connectionFailed && !$connected}
		<!-- Connection failed - server not running -->
		<div class="connection-overlay">
			<div class="connection-card error">
				<div class="error-icon">!</div>
				<h2>Server Not Connected</h2>
				<p>Cannot connect to the Outer server at port 3000.</p>
				<div class="help-text">
					<p>Make sure the server is running:</p>
					<code>cargo run</code>
					<p class="hint">Then refresh this page.</p>
				</div>
			</div>
		</div>
	{:else if !$connected}
		<!-- Lost connection - reconnecting -->
		<div class="connection-status">
			<div class="spinner"></div>
			Reconnecting to server...
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

	.connection-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.connection-card {
		background: var(--color-bg-secondary);
		padding: 32px 48px;
		border-radius: var(--radius-lg);
		text-align: center;
		max-width: 400px;
	}

	.connection-card.error {
		border: 2px solid var(--color-error);
	}

	.connection-card h2 {
		margin: 16px 0 8px;
		color: var(--color-text);
	}

	.connection-card p {
		color: var(--color-text-muted);
		margin: 8px 0;
	}

	.error-icon {
		width: 48px;
		height: 48px;
		border-radius: 50%;
		background: var(--color-error);
		color: white;
		font-size: 24px;
		font-weight: bold;
		display: flex;
		align-items: center;
		justify-content: center;
		margin: 0 auto;
	}

	.help-text {
		margin-top: 24px;
		padding: 16px;
		background: var(--color-bg-tertiary);
		border-radius: var(--radius-md);
	}

	.help-text code {
		display: block;
		background: var(--color-bg);
		padding: 8px 16px;
		border-radius: var(--radius-sm);
		font-family: monospace;
		margin: 8px 0;
	}

	.help-text .hint {
		font-size: 0.9em;
		color: var(--color-text-muted);
	}

	.spinner-large {
		width: 48px;
		height: 48px;
		border: 4px solid var(--color-border);
		border-top-color: var(--color-primary);
		border-radius: 50%;
		animation: spin 1s linear infinite;
		margin: 0 auto;
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
