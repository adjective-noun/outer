<script lang="ts">
	import { journals, createJournal, connected } from '$lib/stores';

	let newTitle = '';
	let creating = false;

	function handleCreate() {
		if (!newTitle.trim()) return;
		creating = true;
		createJournal(newTitle.trim());
		newTitle = '';
		creating = false;
	}
</script>

<svelte:head>
	<title>Outer - Journals</title>
</svelte:head>

<div class="page">
	<header class="header">
		<h1>Outer</h1>
		<p class="subtitle">Collaborative AI Conversations</p>
	</header>

	<main class="main">
		<section class="create-section">
			<form on:submit|preventDefault={handleCreate} class="create-form">
				<input
					type="text"
					bind:value={newTitle}
					placeholder="New journal title..."
					disabled={!$connected || creating}
				/>
				<button
					type="submit"
					class="primary"
					disabled={!$connected || creating || !newTitle.trim()}
				>
					Create
				</button>
			</form>
		</section>

		<section class="journals-section">
			<h2>Journals</h2>
			{#if $journals.length === 0}
				<p class="empty">No journals yet. Create one to get started.</p>
			{:else}
				<ul class="journal-list">
					{#each $journals as journal (journal.id)}
						<li>
							<a href="/journal/{journal.id}" class="journal-card">
								<h3>{journal.title}</h3>
								<span class="date">{new Date(journal.updated_at).toLocaleDateString()}</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</main>
</div>

<style>
	.page {
		min-height: 100%;
		display: flex;
		flex-direction: column;
	}

	.header {
		text-align: center;
		padding: 48px 16px 32px;
		border-bottom: 1px solid var(--color-border);
	}

	.header h1 {
		font-size: 2.5rem;
		font-weight: 700;
		color: var(--color-text-bright);
		margin-bottom: 8px;
	}

	.subtitle {
		color: var(--color-text-muted);
	}

	.main {
		flex: 1;
		max-width: 600px;
		width: 100%;
		margin: 0 auto;
		padding: 32px 16px;
	}

	.create-section {
		margin-bottom: 48px;
	}

	.create-form {
		display: flex;
		gap: 12px;
	}

	.create-form input {
		flex: 1;
	}

	.journals-section h2 {
		font-size: 1.25rem;
		margin-bottom: 16px;
		color: var(--color-text-muted);
	}

	.empty {
		color: var(--color-text-muted);
		text-align: center;
		padding: 32px;
	}

	.journal-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.journal-card {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 16px 20px;
		background: var(--color-bg-secondary);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		transition:
			border-color 0.15s,
			background 0.15s;
	}

	.journal-card:hover {
		border-color: var(--color-primary);
		background: var(--color-bg-tertiary);
		text-decoration: none;
	}

	.journal-card h3 {
		font-size: 1rem;
		font-weight: 500;
		color: var(--color-text-bright);
	}

	.date {
		font-size: 0.875rem;
		color: var(--color-text-muted);
	}

	@media (max-width: 640px) {
		.header {
			padding: 32px 16px 24px;
		}

		.header h1 {
			font-size: 2rem;
		}

		.create-form {
			flex-direction: column;
		}

		.create-form button {
			width: 100%;
		}
	}
</style>
