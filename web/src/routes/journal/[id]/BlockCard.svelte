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
	$: isComplete = block.status === 'complete';

	// Cell type label
	$: cellType = isUser ? 'Input' : 'Output';

	// Normalize whitespace: collapse multiple blank lines into one, trim trailing spaces
	$: normalizedContent = block.content
		.replace(/[ \t]+$/gm, '') // Trim trailing spaces from each line
		.replace(/\n{3,}/g, '\n\n') // Collapse 3+ newlines to 2
		.trim(); // Trim start/end

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

<article class="cell" class:user={isUser} class:assistant={!isUser} class:streaming={isStreaming} class:error={isError} class:forked={isForked} class:optimistic={isOptimistic}>
	<div class="cell-gutter">
		<span class="cell-type" class:user={isUser} class:assistant={!isUser}>{cellType}</span>
		{#if isForked}
			<span class="cell-badge forked">forked</span>
		{/if}
		{#if isOptimistic}
			<span class="cell-badge syncing">syncing</span>
		{/if}
	</div>

	<div class="cell-main">
		<div class="cell-toolbar">
			{#if isComplete}
				<button class="toolbar-btn" on:click={handleFork} title="Fork from this point">
					<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
						<path d="M5 5.372v.878c0 .414.336.75.75.75h4.5a.75.75 0 0 0 .75-.75v-.878a2.25 2.25 0 1 1 1.5 0v.878a2.25 2.25 0 0 1-2.25 2.25h-1.5v2.128a2.251 2.251 0 1 1-1.5 0V8.5h-1.5A2.25 2.25 0 0 1 3.5 6.25v-.878a2.25 2.25 0 1 1 1.5 0ZM5 3.25a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Zm6.75.75a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm-3 8.75a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Z"/>
					</svg>
					Fork
				</button>
				{#if !isUser}
					<button class="toolbar-btn" on:click={handleRerun} title="Re-run this response">
						<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
							<path
								d="M1.705 8.005a.75.75 0 0 1 .834.656 5.5 5.5 0 0 0 9.592 2.97l-1.204-1.204a.25.25 0 0 1 .177-.427h3.646a.25.25 0 0 1 .25.25v3.646a.25.25 0 0 1-.427.177l-1.38-1.38A7.002 7.002 0 0 1 1.05 8.84a.75.75 0 0 1 .656-.834ZM8 2.5a5.487 5.487 0 0 0-4.131 1.869l1.204 1.204A.25.25 0 0 1 4.896 6H1.25A.25.25 0 0 1 1 5.75V2.104a.25.25 0 0 1 .427-.177l1.38 1.38A7.002 7.002 0 0 1 14.95 7.16a.75.75 0 0 1-1.49.178A5.5 5.5 0 0 0 8 2.5Z"
							/>
						</svg>
						Re-run
					</button>
				{/if}
			{:else if isStreaming}
				<button class="toolbar-btn cancel" on:click={handleCancel}>
					Cancel
				</button>
			{/if}
		</div>

		<div class="cell-content">
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
	</div>
</article>

<style>
	/* Observable/Jupyter-style cell layout */
	.cell {
		display: flex;
		border-top: 1px solid var(--color-border);
		background: var(--color-bg);
		transition: background 0.15s;
	}

	.cell:last-child {
		border-bottom: 1px solid var(--color-border);
	}

	.cell:hover {
		background: var(--color-bg-secondary);
	}

	.cell.streaming {
		background: var(--color-bg-secondary);
		border-left: 3px solid var(--color-primary);
	}

	.cell.error {
		border-left: 3px solid var(--color-error);
	}

	.cell.forked {
		border-left: 3px solid var(--color-warning);
	}

	.cell.optimistic {
		opacity: 0.7;
	}

	/* Left gutter with cell type indicator */
	.cell-gutter {
		flex-shrink: 0;
		width: 72px;
		padding: 12px 8px 12px 12px;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 4px;
		border-right: 1px solid var(--color-border);
		background: var(--color-bg-secondary);
	}

	.cell-type {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-text-muted);
	}

	.cell-type.user {
		color: var(--color-user);
	}

	.cell-type.assistant {
		color: var(--color-assistant);
	}

	.cell-badge {
		font-size: 0.5625rem;
		padding: 1px 4px;
		border-radius: 2px;
		text-transform: uppercase;
		font-weight: 600;
	}

	.cell-badge.forked {
		background: var(--color-warning);
		color: var(--color-bg);
	}

	.cell-badge.syncing {
		background: var(--color-text-muted);
		color: var(--color-bg);
		animation: syncPulse 1.5s ease-in-out infinite;
	}

	@keyframes syncPulse {
		0%, 100% { opacity: 0.7; }
		50% { opacity: 1; }
	}

	/* Main content area */
	.cell-main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	/* Toolbar at top of cell */
	.cell-toolbar {
		display: flex;
		gap: 4px;
		padding: 6px 12px;
		min-height: 32px;
		align-items: center;
		justify-content: flex-end;
		border-bottom: 1px solid transparent;
	}

	.cell:hover .cell-toolbar {
		border-bottom-color: var(--color-border);
	}

	.toolbar-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.6875rem;
		padding: 3px 8px;
		background: var(--color-bg-tertiary);
		color: var(--color-text-muted);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		opacity: 0;
		transition: opacity 0.15s, background 0.15s;
	}

	.cell:hover .toolbar-btn,
	.cell.streaming .toolbar-btn {
		opacity: 1;
	}

	.toolbar-btn:hover {
		color: var(--color-text-bright);
		background: var(--color-border);
	}

	.toolbar-btn.cancel {
		color: var(--color-error);
		border-color: var(--color-error);
		opacity: 1;
	}

	.toolbar-btn.cancel:hover {
		background: var(--color-error);
		color: white;
	}

	/* Cell content */
	.cell-content {
		padding: 12px 16px 16px;
		white-space: pre-wrap;
		word-break: break-word;
		min-height: 40px;
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
		0%,
		80%,
		100% {
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
		50% { opacity: 0; }
	}

	.error-content {
		color: var(--color-error);
	}

	@media (max-width: 640px) {
		.cell-gutter {
			width: 56px;
			padding: 10px 6px 10px 8px;
		}

		.cell-type {
			font-size: 0.625rem;
		}

		.cell-content {
			padding: 10px 12px 14px;
		}

		.cell-toolbar {
			padding: 4px 8px;
		}
	}
</style>
