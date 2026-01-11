<script lang="ts">
	import type { Participant } from '$lib/types';

	export let participants: Participant[] = [];
	export let currentParticipant: Participant | null = null;

	$: humans = participants.filter(p => p.kind === 'user');
	$: agents = participants.filter(p => p.kind === 'agent');

	function getStatusColor(status: Participant['status']): string {
		switch (status) {
			case 'active': return 'var(--color-success)';
			case 'idle': return 'var(--color-warning)';
			case 'away': return 'var(--color-text-muted)';
			default: return 'var(--color-text-muted)';
		}
	}
</script>

<div class="presence-bar">
	<div class="presence-group">
		<span class="group-label">Participants</span>
		<div class="avatars">
			{#each participants as participant (participant.id)}
				<div
					class="avatar"
					class:current={participant.id === currentParticipant?.id}
					class:agent={participant.kind === 'agent'}
					style="--color: {participant.color}; --status-color: {getStatusColor(participant.status)}"
					title="{participant.name} ({participant.kind}) - {participant.status}"
				>
					{participant.name.charAt(0).toUpperCase()}
					<span class="status-dot"></span>
				</div>
			{/each}
		</div>
	</div>

	<div class="presence-stats">
		{#if humans.length > 0}
			<span class="stat humans">
				<span class="stat-icon user"></span>
				{humans.length}
			</span>
		{/if}
		{#if agents.length > 0}
			<span class="stat agents">
				<span class="stat-icon agent"></span>
				{agents.length}
			</span>
		{/if}
	</div>
</div>

<style>
	.presence-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 16px;
		background: var(--color-bg-tertiary);
		border-bottom: 1px solid var(--color-border);
		font-size: 0.75rem;
	}

	.presence-group {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.group-label {
		color: var(--color-text-muted);
	}

	.avatars {
		display: flex;
		gap: 4px;
	}

	.avatar {
		position: relative;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: var(--color);
		color: white;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.75rem;
		font-weight: 600;
		border: 2px solid transparent;
		cursor: default;
	}

	.avatar.current {
		border-color: var(--color-primary);
	}

	.avatar.agent {
		border-radius: 6px;
	}

	.status-dot {
		position: absolute;
		bottom: -2px;
		right: -2px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--status-color);
		border: 2px solid var(--color-bg-tertiary);
	}

	.presence-stats {
		display: flex;
		gap: 12px;
	}

	.stat {
		display: flex;
		align-items: center;
		gap: 4px;
		color: var(--color-text-muted);
	}

	.stat-icon {
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.stat-icon.user {
		background: var(--color-user);
	}

	.stat-icon.agent {
		background: var(--color-agent);
		border-radius: 2px;
	}

	@media (max-width: 640px) {
		.presence-bar {
			padding: 6px 12px;
		}

		.group-label {
			display: none;
		}

		.avatar {
			width: 24px;
			height: 24px;
			font-size: 0.625rem;
		}
	}
</style>
