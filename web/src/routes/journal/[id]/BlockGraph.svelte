<script lang="ts">
	import { onMount, onDestroy, createEventDispatcher } from 'svelte';
	import * as d3 from 'd3';
	import type { Block } from '$lib/types';

	export let blocks: Block[] = [];
	export let selectedBlockId: string | null = null;

	const dispatch = createEventDispatcher<{ selectBlock: string }>();

	let svgElement: SVGSVGElement;
	let containerElement: HTMLDivElement;

	// Graph dimensions
	const nodeRadius = 8;
	const levelHeight = 40;
	const branchWidth = 60;

	interface GraphNode {
		id: string;
		block: Block;
		x: number;
		y: number;
		level: number;
		branch: number;
	}

	interface GraphEdge {
		source: GraphNode;
		target: GraphNode;
		type: 'parent' | 'fork';
	}

	// Build graph structure from blocks
	function buildGraph(blocks: Block[]): { nodes: GraphNode[]; edges: GraphEdge[] } {
		if (blocks.length === 0) return { nodes: [], edges: [] };

		const nodeMap = new Map<string, GraphNode>();
		const edges: GraphEdge[] = [];

		// First pass: create nodes and determine levels based on conversation flow
		// Track which blocks are forks and their fork positions
		const forkMap = new Map<string, string[]>(); // original_id -> [forked_ids]

		blocks.forEach(block => {
			if (block.forked_from_id) {
				const forks = forkMap.get(block.forked_from_id) || [];
				forks.push(block.id);
				forkMap.set(block.forked_from_id, forks);
			}
		});

		// Calculate levels: forked blocks appear at the same level as what they forked from
		// This creates a branching structure
		const levelMap = new Map<string, number>();
		const branchMap = new Map<string, number>();
		let maxBranch = 0;

		// Find root blocks (no parent_id and not forked)
		const roots = blocks.filter(b => !b.parent_id && !b.forked_from_id);

		// BFS to assign levels
		function assignLevels(startBlocks: Block[], startLevel: number, branch: number) {
			const queue: { block: Block; level: number; branch: number }[] =
				startBlocks.map(b => ({ block: b, level: startLevel, branch }));

			while (queue.length > 0) {
				const { block, level, branch: currentBranch } = queue.shift()!;

				if (levelMap.has(block.id)) continue;

				levelMap.set(block.id, level);
				branchMap.set(block.id, currentBranch);

				// Find children (blocks with this as parent)
				const children = blocks.filter(b => b.parent_id === block.id && !b.forked_from_id);
				children.forEach(child => {
					queue.push({ block: child, level: level + 1, branch: currentBranch });
				});

				// Handle forks: they branch off at the same level as their successor
				const forks = forkMap.get(block.id) || [];
				forks.forEach((forkId, forkIndex) => {
					const forkedBlock = blocks.find(b => b.id === forkId);
					if (forkedBlock) {
						maxBranch++;
						// Fork appears at level + 1 (same as where the next message would be)
						queue.push({ block: forkedBlock, level: level + 1, branch: maxBranch });
					}
				});
			}
		}

		// Start from roots on branch 0
		assignLevels(roots, 0, 0);

		// Handle any orphaned blocks (shouldn't happen but safety)
		blocks.forEach(block => {
			if (!levelMap.has(block.id)) {
				levelMap.set(block.id, 0);
				branchMap.set(block.id, 0);
			}
		});

		// Create nodes
		blocks.forEach(block => {
			const level = levelMap.get(block.id) || 0;
			const branch = branchMap.get(block.id) || 0;

			nodeMap.set(block.id, {
				id: block.id,
				block,
				level,
				branch,
				x: 30 + branch * branchWidth,
				y: 20 + level * levelHeight
			});
		});

		// Create edges
		blocks.forEach(block => {
			const targetNode = nodeMap.get(block.id);
			if (!targetNode) return;

			// Parent edge (conversation flow)
			if (block.parent_id) {
				const sourceNode = nodeMap.get(block.parent_id);
				if (sourceNode) {
					edges.push({ source: sourceNode, target: targetNode, type: 'parent' });
				}
			}

			// Fork edge
			if (block.forked_from_id) {
				const sourceNode = nodeMap.get(block.forked_from_id);
				if (sourceNode) {
					edges.push({ source: sourceNode, target: targetNode, type: 'fork' });
				}
			}
		});

		return { nodes: Array.from(nodeMap.values()), edges };
	}

	function renderGraph() {
		if (!svgElement || !containerElement) return;

		const { nodes, edges } = buildGraph(blocks);

		// Calculate SVG dimensions
		const maxLevel = Math.max(0, ...nodes.map(n => n.level));
		const maxBranch = Math.max(0, ...nodes.map(n => n.branch));
		const svgWidth = Math.max(80, 60 + maxBranch * branchWidth);
		const svgHeight = Math.max(60, 40 + (maxLevel + 1) * levelHeight);

		const svg = d3.select(svgElement)
			.attr('width', svgWidth)
			.attr('height', svgHeight);

		// Clear previous content
		svg.selectAll('*').remove();

		// Add defs for markers (arrowheads)
		const defs = svg.append('defs');

		// Arrow marker for parent edges
		defs.append('marker')
			.attr('id', 'arrow-parent')
			.attr('viewBox', '0 -5 10 10')
			.attr('refX', 15)
			.attr('refY', 0)
			.attr('markerWidth', 4)
			.attr('markerHeight', 4)
			.attr('orient', 'auto')
			.append('path')
			.attr('d', 'M0,-5L10,0L0,5')
			.attr('fill', 'var(--color-text-muted)');

		// Arrow marker for fork edges
		defs.append('marker')
			.attr('id', 'arrow-fork')
			.attr('viewBox', '0 -5 10 10')
			.attr('refX', 15)
			.attr('refY', 0)
			.attr('markerWidth', 4)
			.attr('markerHeight', 4)
			.attr('orient', 'auto')
			.append('path')
			.attr('d', 'M0,-5L10,0L0,5')
			.attr('fill', 'var(--color-warning)');

		// Draw edges
		const edgeGroup = svg.append('g').attr('class', 'edges');

		edges.forEach(edge => {
			const isFork = edge.type === 'fork';

			// Create path with curve for forks
			const path = edgeGroup.append('path')
				.attr('d', () => {
					if (isFork || edge.source.branch !== edge.target.branch) {
						// Curved path for forks or cross-branch connections
						const midY = (edge.source.y + edge.target.y) / 2;
						return `M${edge.source.x},${edge.source.y}
								Q${edge.source.x},${midY} ${(edge.source.x + edge.target.x) / 2},${midY}
								Q${edge.target.x},${midY} ${edge.target.x},${edge.target.y}`;
					} else {
						// Straight line for same-branch connections
						return `M${edge.source.x},${edge.source.y} L${edge.target.x},${edge.target.y}`;
					}
				})
				.attr('fill', 'none')
				.attr('stroke', isFork ? 'var(--color-warning)' : 'var(--color-text-muted)')
				.attr('stroke-width', isFork ? 2 : 1.5)
				.attr('stroke-dasharray', isFork ? '4,2' : 'none')
				.attr('marker-end', isFork ? 'url(#arrow-fork)' : 'url(#arrow-parent)');
		});

		// Draw nodes
		const nodeGroup = svg.append('g').attr('class', 'nodes');

		nodes.forEach(node => {
			const isUser = node.block.block_type === 'user';
			const isSelected = node.block.id === selectedBlockId;
			const isStreaming = node.block.status === 'streaming';
			const isError = node.block.status === 'error';
			const isForked = !!node.block.forked_from_id;

			const g = nodeGroup.append('g')
				.attr('transform', `translate(${node.x}, ${node.y})`)
				.attr('cursor', 'pointer')
				.on('click', () => {
					dispatch('selectBlock', node.block.id);
				});

			// Node circle
			g.append('circle')
				.attr('r', isSelected ? nodeRadius + 2 : nodeRadius)
				.attr('fill', () => {
					if (isError) return 'var(--color-error)';
					if (isStreaming) return 'var(--color-primary)';
					return isUser ? 'var(--color-user)' : 'var(--color-assistant)';
				})
				.attr('stroke', isSelected ? 'var(--color-text-bright)' : (isForked ? 'var(--color-warning)' : 'none'))
				.attr('stroke-width', isSelected ? 3 : (isForked ? 2 : 0));

			// Streaming animation
			if (isStreaming) {
				g.append('circle')
					.attr('r', nodeRadius + 4)
					.attr('fill', 'none')
					.attr('stroke', 'var(--color-primary)')
					.attr('stroke-width', 2)
					.attr('opacity', 0.5)
					.append('animate')
					.attr('attributeName', 'r')
					.attr('from', nodeRadius + 2)
					.attr('to', nodeRadius + 10)
					.attr('dur', '1s')
					.attr('repeatCount', 'indefinite');

				g.append('circle')
					.attr('r', nodeRadius + 4)
					.attr('fill', 'none')
					.attr('stroke', 'var(--color-primary)')
					.attr('stroke-width', 2)
					.append('animate')
					.attr('attributeName', 'opacity')
					.attr('from', '0.5')
					.attr('to', '0')
					.attr('dur', '1s')
					.attr('repeatCount', 'indefinite');
			}

			// Tooltip on hover
			g.append('title')
				.text(() => {
					const type = isUser ? 'User' : 'Assistant';
					const status = node.block.status;
					const preview = node.block.content.slice(0, 50) + (node.block.content.length > 50 ? '...' : '');
					return `${type} (${status})\n${preview}`;
				});
		});
	}

	// Re-render when blocks or selection changes
	$: if (svgElement && containerElement) {
		renderGraph();
	}

	onMount(() => {
		renderGraph();
	});
</script>

<div class="block-graph" bind:this={containerElement}>
	<div class="graph-header">
		<span class="graph-title">Block Graph</span>
		<div class="legend">
			<span class="legend-item">
				<span class="dot user"></span>
				User
			</span>
			<span class="legend-item">
				<span class="dot assistant"></span>
				Assistant
			</span>
		</div>
	</div>
	<div class="graph-container">
		<svg bind:this={svgElement}></svg>
	</div>
</div>

<style>
	.block-graph {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--color-bg-secondary);
		border-right: 1px solid var(--color-border);
	}

	.graph-header {
		padding: 12px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg-tertiary);
	}

	.graph-title {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--color-text-bright);
	}

	.legend {
		display: flex;
		gap: 12px;
		margin-top: 8px;
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.dot.user {
		background: var(--color-user);
	}

	.dot.assistant {
		background: var(--color-assistant);
	}

	.graph-container {
		flex: 1;
		overflow: auto;
		padding: 8px;
	}

	.graph-container svg {
		display: block;
	}
</style>
