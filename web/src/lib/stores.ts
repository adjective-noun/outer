import { writable, derived, get } from 'svelte/store';
import type { Journal, Block, Participant, WorkItem, ApprovalRequest, ServerMessage } from './types';
import { getWebSocketClient } from './websocket';

// Connection state
export const connected = writable(false);
export const error = writable<string | null>(null);

// Journals
export const journals = writable<Journal[]>([]);
export const currentJournalId = writable<string | null>(null);
export const currentJournal = derived(
	[journals, currentJournalId],
	([$journals, $id]) => $journals.find((j) => j.id === $id) ?? null
);

// Blocks for current journal
export const blocks = writable<Block[]>([]);

// Participants in current journal
export const participants = writable<Participant[]>([]);
export const currentParticipant = writable<Participant | null>(null);

// My participant ID for delegation
export const myParticipantId = writable<string | null>(null);

// Work items and approvals
export const workQueue = writable<WorkItem[]>([]);
export const approvalQueue = writable<ApprovalRequest[]>([]);
export const availableParticipants = writable<Array<{ id: string; name: string; kind: string; capabilities: string[] }>>([]);

// Session ID for OpenCode
export const sessionId = writable<string | null>(null);

// Pending messages for optimistic UI
interface PendingMessage {
	tempId: string;
	journalId: string;
	content: string;
	timestamp: number;
}
export const pendingMessages = writable<PendingMessage[]>([]);

// Local storage key for pending messages
const PENDING_MESSAGES_KEY = 'outer_pending_messages';

// Load pending messages from localStorage
function loadPendingMessages(): PendingMessage[] {
	try {
		const stored = localStorage.getItem(PENDING_MESSAGES_KEY);
		if (stored) {
			return JSON.parse(stored);
		}
	} catch (e) {
		console.error('Failed to load pending messages:', e);
	}
	return [];
}

// Save pending messages to localStorage
function savePendingMessages(messages: PendingMessage[]) {
	try {
		localStorage.setItem(PENDING_MESSAGES_KEY, JSON.stringify(messages));
	} catch (e) {
		console.error('Failed to save pending messages:', e);
	}
}

// Initialize pending messages from localStorage
if (typeof window !== 'undefined') {
	pendingMessages.set(loadPendingMessages());
	pendingMessages.subscribe((msgs) => savePendingMessages(msgs));
}

// Initialize WebSocket and handlers
export function initializeConnection(): Promise<void> {
	const ws = getWebSocketClient();

	// Register connection state callbacks BEFORE connecting
	ws.onConnect(() => {
		connected.set(true);
		error.set(null);
		// Load journals on connect/reconnect
		ws.send({ type: 'list_journals' });

		// Resend pending messages on reconnect
		const pending = get(pendingMessages);
		const currentJournal = get(currentJournalId);
		if (currentJournal && pending.length > 0) {
			// Re-subscribe to the current journal
			const storedName = typeof localStorage !== 'undefined'
				? localStorage.getItem('outer_user_name') || 'User'
				: 'User';
			ws.send({ type: 'subscribe', journal_id: currentJournal, name: storedName, kind: 'user' });

			// Resend any pending messages for this journal
			const journalPending = pending.filter(p => p.journalId === currentJournal);
			for (const msg of journalPending) {
				const sid = get(sessionId);
				ws.send({
					type: 'submit',
					journal_id: msg.journalId,
					content: msg.content,
					session_id: sid ?? undefined
				});
			}
		}

		// Reload current journal blocks to fix any stale pending states
		if (currentJournal) {
			ws.send({ type: 'get_journal', journal_id: currentJournal });
		}
	});

	ws.onDisconnect(() => {
		connected.set(false);
	});

	ws.onError((err) => {
		error.set(err);
		setTimeout(() => error.set(null), 5000);
	});

	ws.subscribe((message: ServerMessage) => {
		switch (message.type) {
			case 'journals':
				journals.set(message.journals);
				break;

			case 'journal_created':
				journals.update((js) => [
					...js,
					{
						id: message.journal_id,
						title: message.title,
						created_at: new Date().toISOString(),
						updated_at: new Date().toISOString()
					}
				]);
				break;

			case 'journal':
				journals.update((js) => {
					const idx = js.findIndex((j) => j.id === message.journal.id);
					if (idx >= 0) {
						js[idx] = message.journal;
						return [...js];
					}
					return [...js, message.journal];
				});
				blocks.set(message.blocks);
				break;

			case 'block_created':
				// Check if this is a user block that matches a pending message
				if (message.block.block_type === 'user') {
					const pending = get(pendingMessages);
					const matchIdx = pending.findIndex(p =>
						p.journalId === message.block.journal_id &&
						p.content === message.block.content
					);
					if (matchIdx >= 0) {
						// Remove the pending message
						pendingMessages.update(ps => ps.filter((_, i) => i !== matchIdx));
						// Replace optimistic block with real one
						blocks.update((bs) => {
							const optimisticIdx = bs.findIndex(b =>
								b.id.startsWith('pending-') &&
								b.content === message.block.content
							);
							if (optimisticIdx >= 0) {
								// Replace optimistic with real
								const newBlocks = [...bs];
								newBlocks[optimisticIdx] = message.block;
								return newBlocks;
							}
							// No optimistic block found, just add
							return [...bs, message.block];
						});
						break;
					}
				}
				blocks.update((bs) => [...bs, message.block]);
				break;

			case 'block_content_delta':
				blocks.update((bs) => {
					const idx = bs.findIndex((b) => b.id === message.block_id);
					if (idx >= 0) {
						bs[idx] = { ...bs[idx], content: bs[idx].content + message.delta };
						return [...bs];
					}
					return bs;
				});
				break;

			case 'block_status_changed':
				blocks.update((bs) => {
					const idx = bs.findIndex((b) => b.id === message.block_id);
					if (idx >= 0) {
						bs[idx] = { ...bs[idx], status: message.status };
						return [...bs];
					}
					return bs;
				});
				break;

			case 'block_forked':
				blocks.update((bs) => [...bs, message.new_block]);
				break;

			case 'block_cancelled':
				blocks.update((bs) => {
					const idx = bs.findIndex((b) => b.id === message.block_id);
					if (idx >= 0) {
						bs[idx] = { ...bs[idx], status: 'error' };
						return [...bs];
					}
					return bs;
				});
				break;

			case 'subscribed':
				currentParticipant.set(message.participant);
				participants.set(message.participants);
				break;

			case 'participant_joined':
				participants.update((ps) => [...ps, message.participant]);
				break;

			case 'participant_left':
				participants.update((ps) => ps.filter((p) => p.id !== message.participant_id));
				break;

			case 'cursor_moved':
				participants.update((ps) => {
					const idx = ps.findIndex((p) => p.id === message.participant_id);
					if (idx >= 0) {
						ps[idx] = {
							...ps[idx],
							cursor_block_id: message.block_id,
							cursor_offset: message.offset
						};
						return [...ps];
					}
					return ps;
				});
				break;

			case 'participant_status_changed':
				participants.update((ps) => {
					const idx = ps.findIndex((p) => p.id === message.participant_id);
					if (idx >= 0) {
						ps[idx] = { ...ps[idx], status: message.status };
						return [...ps];
					}
					return ps;
				});
				break;

			case 'presence':
				participants.set(message.participants);
				break;

			case 'participant_registered':
				myParticipantId.set(message.participant_id);
				break;

			case 'work_queue':
				workQueue.set(message.items);
				break;

			case 'approval_queue':
				approvalQueue.set(message.items);
				break;

			case 'available_participants':
				availableParticipants.set(message.participants);
				break;

			case 'work_delegated':
				workQueue.update((wq) => [...wq, message.work_item]);
				break;

			case 'work_accepted':
			case 'work_declined':
			case 'work_cancelled':
			case 'work_claimed':
				// Refresh work queue
				ws.send({ type: 'get_work_queue' });
				break;

			case 'approval_requested':
				approvalQueue.update((aq) => [...aq, message.approval]);
				break;

			case 'work_approved':
			case 'work_rejected':
				// Refresh approval queue
				ws.send({ type: 'get_approval_queue' });
				ws.send({ type: 'get_work_queue' });
				break;

			case 'error':
				// Log full error details to console for debugging
				if (message.details) {
					console.error('Server error details:', message.details);
				} else {
					console.error('Server error:', message.message);
				}
				// Show user-friendly message in toast
				error.set(message.message);
				setTimeout(() => error.set(null), 5000);
				break;
		}
	});

	return ws.connect();
}

// Actions
export function createJournal(title?: string) {
	getWebSocketClient().send({ type: 'create_journal', title });
}

export function loadJournal(journalId: string) {
	currentJournalId.set(journalId);
	getWebSocketClient().send({ type: 'get_journal', journal_id: journalId });
}

export function subscribeToJournal(journalId: string, name: string, kind: 'user' | 'agent' = 'user') {
	getWebSocketClient().send({ type: 'subscribe', journal_id: journalId, name, kind });
}

export function unsubscribeFromJournal(journalId: string) {
	getWebSocketClient().send({ type: 'unsubscribe', journal_id: journalId });
}

export function submitPrompt(journalId: string, content: string) {
	const sid = get(sessionId);
	const tempId = `pending-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

	// Add to pending messages for durability
	const pendingMsg: PendingMessage = {
		tempId,
		journalId,
		content,
		timestamp: Date.now()
	};
	pendingMessages.update(ps => [...ps, pendingMsg]);

	// Add optimistic block immediately
	const optimisticBlock: Block = {
		id: tempId,
		journal_id: journalId,
		block_type: 'user',
		content,
		status: 'pending',
		created_at: new Date().toISOString(),
		updated_at: new Date().toISOString()
	};
	blocks.update(bs => [...bs, optimisticBlock]);

	// Send to server
	getWebSocketClient().send({
		type: 'submit',
		journal_id: journalId,
		content,
		session_id: sid ?? undefined
	});
}

export function forkBlock(blockId: string) {
	const sid = get(sessionId);
	getWebSocketClient().send({
		type: 'fork',
		block_id: blockId,
		session_id: sid ?? undefined
	});
}

export function rerunBlock(blockId: string) {
	const sid = get(sessionId);
	getWebSocketClient().send({
		type: 'rerun',
		block_id: blockId,
		session_id: sid ?? undefined
	});
}

export function cancelBlock(blockId: string) {
	getWebSocketClient().send({ type: 'cancel', block_id: blockId });
}

export function updateCursor(journalId: string, blockId?: string, offset?: number) {
	getWebSocketClient().send({ type: 'cursor', journal_id: journalId, block_id: blockId, offset });
}

// Delegation actions
export function registerAsParticipant(journalId: string, name: string, kind: 'user' | 'agent' = 'user', capabilities?: string[]) {
	getWebSocketClient().send({
		type: 'register_participant',
		journal_id: journalId,
		name,
		kind,
		capabilities
	});
}

export function delegateWork(journalId: string, description: string, assigneeId: string, options?: {
	priority?: string;
	requiresApproval?: boolean;
	approverId?: string;
}) {
	getWebSocketClient().send({
		type: 'delegate',
		journal_id: journalId,
		description,
		assignee_id: assigneeId,
		priority: options?.priority,
		requires_approval: options?.requiresApproval,
		approver_id: options?.approverId
	});
}

export function acceptWork(workItemId: string) {
	getWebSocketClient().send({ type: 'accept_work', work_item_id: workItemId });
}

export function declineWork(workItemId: string) {
	getWebSocketClient().send({ type: 'decline_work', work_item_id: workItemId });
}

export function submitWork(workItemId: string, result: string) {
	getWebSocketClient().send({ type: 'submit_work', work_item_id: workItemId, result });
}

export function approveWork(approvalId: string, feedback?: string) {
	getWebSocketClient().send({ type: 'approve_work', approval_id: approvalId, feedback });
}

export function rejectWork(approvalId: string, feedback: string) {
	getWebSocketClient().send({ type: 'reject_work', approval_id: approvalId, feedback });
}

export function claimWork(workItemId: string) {
	getWebSocketClient().send({ type: 'claim_work', work_item_id: workItemId });
}

export function loadWorkQueue() {
	getWebSocketClient().send({ type: 'get_work_queue' });
}

export function loadApprovalQueue() {
	getWebSocketClient().send({ type: 'get_approval_queue' });
}

export function loadAvailableParticipants(journalId: string) {
	getWebSocketClient().send({ type: 'get_participants', journal_id: journalId });
}
