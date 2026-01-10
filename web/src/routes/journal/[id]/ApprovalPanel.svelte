<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { ApprovalRequest, WorkItem } from '$lib/types';
	import { approveWork, rejectWork } from '$lib/stores';

	export let approvals: ApprovalRequest[] = [];
	export let workItems: WorkItem[] = [];

	const dispatch = createEventDispatcher();

	$: pendingApprovals = approvals.filter(a => a.status === 'pending');

	let feedbackInputs: Record<string, string> = {};

	function getWorkItem(workItemId: string): WorkItem | undefined {
		return workItems.find(w => w.id === workItemId);
	}

	function handleApprove(approval: ApprovalRequest) {
		approveWork(approval.id, feedbackInputs[approval.id]);
		feedbackInputs[approval.id] = '';
	}

	function handleReject(approval: ApprovalRequest) {
		const feedback = feedbackInputs[approval.id]?.trim();
		if (!feedback) {
			alert('Please provide feedback when rejecting');
			return;
		}
		rejectWork(approval.id, feedback);
		feedbackInputs[approval.id] = '';
	}

	function close() {
		dispatch('close');
	}
</script>

<div class="approval-panel">
	<header class="panel-header">
		<h2>Pending Approvals</h2>
		<button class="close-btn" on:click={close}>&times;</button>
	</header>

	<div class="panel-content">
		{#if pendingApprovals.length === 0}
			<p class="empty">No pending approvals</p>
		{:else}
			{#each pendingApprovals as approval (approval.id)}
				{@const workItem = getWorkItem(approval.work_item_id)}
				<div class="approval-card">
					<div class="approval-info">
						<h3>{workItem?.description ?? 'Work Item'}</h3>
						<span class="priority" class:urgent={workItem?.priority === 'urgent'} class:high={workItem?.priority === 'high'}>
							{workItem?.priority ?? 'normal'}
						</span>
					</div>

					{#if workItem?.result}
						<div class="result-preview">
							<strong>Result:</strong>
							<p>{workItem.result}</p>
						</div>
					{/if}

					<div class="feedback-input">
						<input
							type="text"
							placeholder="Feedback (required for rejection)..."
							bind:value={feedbackInputs[approval.id]}
						/>
					</div>

					<div class="approval-actions">
						<button class="approve-btn" on:click={() => handleApprove(approval)}>
							Approve
						</button>
						<button class="reject-btn" on:click={() => handleReject(approval)}>
							Reject
						</button>
					</div>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.approval-panel {
		position: fixed;
		right: 0;
		top: 0;
		bottom: 0;
		width: 380px;
		max-width: 100%;
		background: var(--color-bg-secondary);
		border-left: 1px solid var(--color-border);
		display: flex;
		flex-direction: column;
		z-index: 100;
		animation: slideIn 0.2s ease-out;
	}

	@keyframes slideIn {
		from {
			transform: translateX(100%);
		}
		to {
			transform: translateX(0);
		}
	}

	.panel-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 16px;
		border-bottom: 1px solid var(--color-border);
	}

	.panel-header h2 {
		font-size: 1rem;
		font-weight: 600;
		color: var(--color-text-bright);
	}

	.close-btn {
		width: 32px;
		height: 32px;
		padding: 0;
		font-size: 1.25rem;
		line-height: 1;
		background: transparent;
		color: var(--color-text-muted);
	}

	.close-btn:hover {
		color: var(--color-text);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.empty {
		text-align: center;
		color: var(--color-text-muted);
		padding: 32px;
	}

	.approval-card {
		background: var(--color-bg-tertiary);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.approval-info {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 12px;
	}

	.approval-info h3 {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-text-bright);
		flex: 1;
	}

	.priority {
		font-size: 0.625rem;
		padding: 2px 6px;
		background: var(--color-bg-secondary);
		color: var(--color-text-muted);
		border-radius: 4px;
		text-transform: uppercase;
		font-weight: 600;
	}

	.priority.urgent {
		background: var(--color-error);
		color: white;
	}

	.priority.high {
		background: var(--color-warning);
		color: var(--color-bg);
	}

	.result-preview {
		font-size: 0.875rem;
		background: var(--color-bg);
		padding: 10px;
		border-radius: var(--radius-sm);
	}

	.result-preview strong {
		color: var(--color-text-muted);
		font-size: 0.75rem;
		text-transform: uppercase;
	}

	.result-preview p {
		margin-top: 4px;
		color: var(--color-text);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.feedback-input input {
		width: 100%;
	}

	.approval-actions {
		display: flex;
		gap: 8px;
	}

	.approve-btn {
		flex: 1;
		background: var(--color-success);
		color: white;
	}

	.approve-btn:hover {
		background: #2ea043;
	}

	.reject-btn {
		flex: 1;
		background: transparent;
		border: 1px solid var(--color-error);
		color: var(--color-error);
	}

	.reject-btn:hover {
		background: var(--color-error);
		color: white;
	}

	@media (max-width: 640px) {
		.approval-panel {
			width: 100%;
		}
	}
</style>
