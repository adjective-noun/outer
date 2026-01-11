<script lang="ts">
	import type { Block } from '$lib/types';
	import { forkBlock, rerunBlock, cancelBlock } from '$lib/stores';

	export let block: Block;

	$: isUser = block.block_type === 'user';
	$: isStreaming = block.status === 'streaming';
	$: isError = block.status === 'error';
	$: isPending = block.status === 'pending';
	$: isForked = !!block.forked_from_id;
	$: isOptimistic = block.id.startsWith('pending-');

	// Find author (for now, infer from block type)
	$: author = isUser ? 'You' : 'Assistant';
	$: authorKind = isUser ? 'user' : 'agent';

	// Normalize whitespace: collapse multiple blank lines into one, trim trailing spaces
	$: normalizedContent = block.content
		.replace(/[ \t]+$/gm, '')           // Trim trailing spaces from each line
		.replace(/\n{3,}/g, '\n\n')         // Collapse 3+ newlines to 2
		.trim();                            // Trim start/end

	function handleFork() {
		forkBlock(block.id);
	}

	function handleRerun() {
		rerunBlock(block.id);
	}

	function handleCancel() {
		cancelBlock(block.id);
	}
</script>

<article class="block-card" class:user={isUser} class:assistant={!isUser} class:streaming={isStreaming} class:error={isError} class:forked={isForked} class:optimistic={isOptimistic}>
	<header class="block-header">
		<div class="author" class:user={isUser} class:agent={!isUser}>
			<span class="author-indicator" class:user={isUser} class:agent={!isUser}></span>
			<span class="author-name">{author}</span>
			{#if isForked}
				<span class="fork-badge">forked</span>
			{/if}
			{#if isOptimistic}
				<span class="syncing-badge">syncing...</span>
			{/if}
		</div>
		<time class="timestamp">{new Date(block.created_at).toLocaleTimeString()}</time>
	</header>

	<div class="block-content">
		{#if isPending && !isOptimistic}
			<div class="pending">
				<span class="dot"></span>
				<span class="dot"></span>
				<span class="dot"></span>
			</div>
		{:else if isError}
			<div class="error-content">
				{block.content || 'An error occurred'}
			</div>
		{:else}
			<div class="content-text">{normalizedContent}</div>
		{/if}

		{#if isStreaming}
			<span class="streaming-cursor">|</span>
		{/if}
	</div>

	<footer class="block-footer">
		{#if block.status === 'complete'}
			<div class="actions">
				{#if isUser}
					<button class="action-btn" on:click={handleFork} title="Fork from this point">
						<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
							<path d="M5 5.372v.878c0 .414.336.75.75.75h4.5a.75.75 0 0 0 .75-.75v-.878a2.25 2.25 0 1 1 1.5 0v.878a2.25 2.25 0 0 1-2.25 2.25h-1.5v2.128a2.251 2.251 0 1 1-1.5 0V8.5h-1.5A2.25 2.25 0 0 1 3.5 6.25v-.878a2.25 2.25 0 1 1 1.5 0ZM5 3.25a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Zm6.75.75a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm-3 8.75a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Z"/>
						</svg>
						Fork
					</button>
				{/if}
				{#if !isUser}
					<button class="action-btn" on:click={handleRerun} title="Re-run this response">
						<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
							<path d="M1.705 8.005a.75.75 0 0 1 .834.656 5.5 5.5 0 0 0 9.592 2.97l-1.204-1.204a.25.25 0 0 1 .177-.427h3.646a.25.25 0 0 1 .25.25v3.646a.25.25 0 0 1-.427.177l-1.38-1.38A7.002 7.002 0 0 1 1.05 8.84a.75.75 0 0 1 .656-.834ZM8 2.5a5.487 5.487 0 0 0-4.131 1.869l1.204 1.204A.25.25 0 0 1 4.896 6H1.25A.25.25 0 0 1 1 5.75V2.104a.25.25 0 0 1 .427-.177l1.38 1.38A7.002 7.002 0 0 1 14.95 7.16a.75.75 0 0 1-1.49.178A5.5 5.5 0 0 0 8 2.5Z"/>
						</svg>
						Re-run
					</button>
				{/if}
			</div>
		{:else if isStreaming}
			<button class="action-btn cancel" on:click={handleCancel}>
				Cancel
			</button>
		{/if}
	</footer>
</article>

<style>
	.block-card {
		background: var(--color-bg-secondary);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
		transition: border-color 0.15s;
	}

	.block-card.streaming {
		border-color: var(--color-primary);
	}

	.block-card.error {
		border-color: var(--color-error);
	}

	.block-card.forked {
		border-left: 3px solid var(--color-warning);
	}

	.block-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 14px;
		background: var(--color-bg-tertiary);
		border-bottom: 1px solid var(--color-border);
	}

	.author {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.875rem;
		font-weight: 500;
	}

	.author-indicator {
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.author-indicator.user {
		background: var(--color-user);
	}

	.author-indicator.agent {
		background: var(--color-assistant);
	}

	.author-name {
		color: var(--color-text-bright);
	}

	.fork-badge {
		font-size: 0.625rem;
		padding: 2px 6px;
		background: var(--color-warning);
		color: var(--color-bg);
		border-radius: 4px;
		text-transform: uppercase;
		font-weight: 600;
	}

	.syncing-badge {
		font-size: 0.625rem;
		padding: 2px 6px;
		background: var(--color-text-muted);
		color: var(--color-bg);
		border-radius: 4px;
		font-weight: 500;
		animation: syncPulse 1.5s ease-in-out infinite;
	}

	@keyframes syncPulse {
		0%, 100% {
			opacity: 0.7;
		}
		50% {
			opacity: 1;
		}
	}

	.block-card.optimistic {
		opacity: 0.85;
		border-style: dashed;
	}

	.timestamp {
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	.block-content {
		padding: 14px;
		min-height: 40px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.content-text {
		line-height: 1.6;
	}

	.pending {
		display: flex;
		gap: 4px;
		padding: 4px 0;
	}

	.pending .dot {
		width: 6px;
		height: 6px;
		background: var(--color-text-muted);
		border-radius: 50%;
		animation: pulse 1.4s ease-in-out infinite;
	}

	.pending .dot:nth-child(2) {
		animation-delay: 0.2s;
	}

	.pending .dot:nth-child(3) {
		animation-delay: 0.4s;
	}

	@keyframes pulse {
		0%, 80%, 100% {
			opacity: 0.3;
			transform: scale(0.8);
		}
		40% {
			opacity: 1;
			transform: scale(1);
		}
	}

	.streaming-cursor {
		color: var(--color-primary);
		animation: blink 1s step-end infinite;
	}

	@keyframes blink {
		50% {
			opacity: 0;
		}
	}

	.error-content {
		color: var(--color-error);
	}

	.block-footer {
		padding: 8px 14px;
		border-top: 1px solid var(--color-border);
		display: flex;
		justify-content: flex-end;
	}

	.actions {
		display: flex;
		gap: 8px;
	}

	.action-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 0.75rem;
		padding: 4px 10px;
		background: transparent;
		color: var(--color-text-muted);
		border: 1px solid var(--color-border);
	}

	.action-btn:hover {
		color: var(--color-text);
		border-color: var(--color-text-muted);
		background: var(--color-bg-tertiary);
	}

	.action-btn.cancel {
		color: var(--color-error);
		border-color: var(--color-error);
	}

	.action-btn.cancel:hover {
		background: var(--color-error);
		color: white;
	}

	@media (max-width: 640px) {
		.block-header {
			padding: 8px 12px;
		}

		.block-content {
			padding: 12px;
		}

		.block-footer {
			padding: 6px 12px;
		}
	}
</style>
