<script lang="ts">
	import { page } from '$app/stores';
	import { goto, invalidateAll } from '$app/navigation';
	import { onMount, onDestroy, tick } from 'svelte';
	import {
		loadJournal,
		subscribeToJournal,
		unsubscribeFromJournal,
		submitPrompt,
		currentJournal,
		blocks,
		participants,
		currentParticipant,
		registerAsParticipant,
		loadWorkQueue,
		loadApprovalQueue,
		workQueue,
		approvalQueue,
		connected
	} from '$lib/stores';
	import BlockCard from './BlockCard.svelte';
	import PresenceBar from './PresenceBar.svelte';
	import ApprovalPanel from './ApprovalPanel.svelte';
	import BlockGraph from './BlockGraph.svelte';

	$: journalId = $page.params.id as string;

	let promptInput = '';
	let submitting = false;
	let scrollContainer: HTMLElement;
	let showApprovals = false;
	let userName = 'User';
	let currentJournalIdRef: string | null = null;
	let userScrolledUp = false;
	let previousBlockCount = 0;
	let selectedBlockId: string | null = null;
	let blockElements: Map<string, HTMLElement> = new Map();

	onMount(() => {
		// Try to get stored name
		const stored = localStorage.getItem('outer_user_name');
		if (stored) userName = stored;

		if (journalId) {
			currentJournalIdRef = journalId;
			loadJournal(journalId);
			subscribeToJournal(journalId, userName, 'user');
			registerAsParticipant(journalId, userName, 'user');
			loadWorkQueue();
			loadApprovalQueue();
		}
	});

	onDestroy(() => {
		if (currentJournalIdRef) {
			unsubscribeFromJournal(currentJournalIdRef);
		}
	});

	function handleSubmit() {
		if (!promptInput.trim() || submitting || !journalId) return;
		submitting = true;
		submitPrompt(journalId, promptInput.trim());
		promptInput = '';
		submitting = false;
	}

	async function handleSelectBlock(event: CustomEvent<string>) {
		const blockId = event.detail;
		selectedBlockId = blockId;

		// Wait for any DOM updates
		await tick();

		// Find the block element and scroll to it
		const blockElement = document.querySelector(`[data-block-id="${blockId}"]`);
		if (blockElement && scrollContainer) {
			blockElement.scrollIntoView({ behavior: 'smooth', block: 'center' });

			// Add a brief highlight effect
			blockElement.classList.add('highlight-flash');
			setTimeout(() => {
				blockElement.classList.remove('highlight-flash');
			}, 1500);
		}
	}

	// Track if user scrolled away from bottom
	function handleScroll() {
		if (!scrollContainer) return;
		const { scrollTop, scrollHeight, clientHeight } = scrollContainer;
		// Consider "at bottom" if within 100px of the bottom
		userScrolledUp = scrollHeight - scrollTop - clientHeight > 100;
	}

	// Auto-scroll only on new blocks and only if user hasn't scrolled up
	$: if ($blocks.length > previousBlockCount && scrollContainer && !userScrolledUp) {
		previousBlockCount = $blocks.length;
		requestAnimationFrame(() => {
			scrollContainer.scrollTop = scrollContainer.scrollHeight;
		});
	} else if ($blocks.length !== previousBlockCount) {
		previousBlockCount = $blocks.length;
	}

	// Count pending approvals
	$: pendingApprovals = $approvalQueue.filter(a => a.status === 'pending').length;

	// Navigate back to journals with proper cleanup
	async function goToJournals() {
		if (currentJournalIdRef) {
			unsubscribeFromJournal(currentJournalIdRef);
		}
		await invalidateAll();
		goto('/');
	}
</script>

<svelte:head>
	<title>{$currentJournal?.title ?? 'Journal'} - Outer</title>
</svelte:head>

<div class="journal-page">
	<header class="header">
		<a href="/" class="back" on:click|preventDefault={goToJournals}>&larr; Journals</a>
		<h1>{$currentJournal?.title ?? 'Loading...'}</h1>
		<div class="header-actions">
			{#if pendingApprovals > 0}
				<button class="approval-badge" on:click={() => showApprovals = !showApprovals}>
					{pendingApprovals} pending
				</button>
			{/if}
		</div>
	</header>

	<PresenceBar participants={$participants} currentParticipant={$currentParticipant} />

	<div class="main-area">
		<aside class="graph-sidebar">
			<BlockGraph blocks={$blocks} {selectedBlockId} on:selectBlock={handleSelectBlock} />
		</aside>

		<main class="content" bind:this={scrollContainer} on:scroll={handleScroll}>
			<div class="blocks-container">
				{#if $blocks.length === 0}
					<div class="empty-state">
						<p>Start a conversation by sending a message below.</p>
					</div>
				{:else}
					{#each $blocks as block (block.id)}
						<div data-block-id={block.id} class="block-wrapper" class:selected={selectedBlockId === block.id}>
							<BlockCard {block} />
						</div>
					{/each}
				{/if}
			</div>
		</main>
	</div>

	{#if showApprovals && $approvalQueue.length > 0}
		<ApprovalPanel
			approvals={$approvalQueue}
			workItems={$workQueue}
			on:close={() => showApprovals = false}
		/>
	{/if}

	<footer class="input-area">
		<form on:submit|preventDefault={handleSubmit} class="input-form">
			<textarea
				bind:value={promptInput}
				placeholder="Type your message..."
				rows="1"
				disabled={!$connected || submitting}
				on:keydown={(e) => {
					if (e.key === 'Enter' && !e.shiftKey) {
						e.preventDefault();
						handleSubmit();
					}
				}}
			></textarea>
			<button type="submit" class="primary" disabled={!$connected || submitting || !promptInput.trim()}>
				Send
			</button>
		</form>
	</footer>
</div>

<style>
	.journal-page {
		height: 100vh;
		display: flex;
		flex-direction: column;
		background: var(--color-bg);
	}

	.header {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg-secondary);
	}

	.back {
		color: var(--color-text-muted);
		font-size: 0.875rem;
	}

	.back:hover {
		color: var(--color-primary);
	}

	.header h1 {
		flex: 1;
		font-size: 1.125rem;
		font-weight: 600;
		color: var(--color-text-bright);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.header-actions {
		display: flex;
		gap: 8px;
	}

	.approval-badge {
		background: var(--color-warning);
		color: var(--color-bg);
		font-size: 0.75rem;
		padding: 4px 10px;
		font-weight: 600;
	}

	.main-area {
		flex: 1;
		display: flex;
		overflow: hidden;
	}

	.graph-sidebar {
		width: 180px;
		min-width: 180px;
		flex-shrink: 0;
		overflow: hidden;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		scroll-behavior: smooth;
	}

	.block-wrapper {
		transition: transform 0.2s, box-shadow 0.2s;
	}

	.block-wrapper.selected {
		transform: scale(1.01);
		box-shadow: 0 0 0 2px var(--color-primary);
		border-radius: var(--radius-md);
	}

	:global(.block-wrapper.highlight-flash) {
		animation: flash 1.5s ease-out;
	}

	@keyframes flash {
		0% {
			box-shadow: 0 0 0 3px var(--color-primary);
		}
		100% {
			box-shadow: none;
		}
	}

	.blocks-container {
		max-width: 800px;
		margin: 0 auto;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.empty-state {
		text-align: center;
		padding: 64px 16px;
		color: var(--color-text-muted);
	}

	.input-area {
		border-top: 1px solid var(--color-border);
		background: var(--color-bg-secondary);
		padding: 12px 16px;
	}

	.input-form {
		max-width: 800px;
		margin: 0 auto;
		display: flex;
		gap: 12px;
		align-items: flex-end;
	}

	.input-form textarea {
		flex: 1;
		resize: none;
		min-height: 44px;
		max-height: 200px;
		line-height: 1.5;
		overflow-y: auto;
	}

	.input-form button {
		height: 44px;
		min-width: 80px;
	}

	@media (max-width: 768px) {
		.graph-sidebar {
			width: 120px;
			min-width: 120px;
		}
	}

	@media (max-width: 640px) {
		.header {
			padding: 10px 12px;
		}

		.header h1 {
			font-size: 1rem;
		}

		.graph-sidebar {
			display: none;
		}

		.blocks-container {
			padding: 12px;
			gap: 12px;
		}

		.input-area {
			padding: 10px 12px;
		}

		.input-form {
			gap: 8px;
		}

		.input-form button {
			min-width: 60px;
			padding: 8px 12px;
		}
	}
</style>
