// Data types matching the Rust server models

export interface Journal {
	id: string;
	title: string;
	created_at: string;
	updated_at: string;
}

export interface Block {
	id: string;
	journal_id: string;
	block_type: 'user' | 'assistant';
	content: string;
	status: 'pending' | 'streaming' | 'complete' | 'error';
	parent_id?: string;
	forked_from_id?: string;
	created_at: string;
	updated_at: string;
}

export interface Participant {
	id: string;
	name: string;
	kind: 'user' | 'agent';
	status: 'active' | 'idle' | 'away';
	color: string;
	cursor_block_id?: string;
	cursor_offset?: number;
	joined_at: string;
}

export interface WorkItem {
	id: string;
	journal_id: string;
	description: string;
	delegator_id: string;
	assignee_id?: string;
	status:
		| 'pending'
		| 'assigned'
		| 'in_progress'
		| 'awaiting_approval'
		| 'completed'
		| 'rejected'
		| 'cancelled';
	priority: 'low' | 'normal' | 'high' | 'urgent';
	result?: string;
	created_at: string;
	updated_at: string;
}

export interface ApprovalRequest {
	id: string;
	work_item_id: string;
	approver_id: string;
	status: 'pending' | 'approved' | 'rejected';
	feedback?: string;
	created_at: string;
}

// Client -> Server messages
export type ClientMessage =
	| { type: 'submit'; journal_id: string; content: string; session_id?: string }
	| { type: 'create_journal'; title?: string }
	| { type: 'get_journal'; journal_id: string }
	| { type: 'list_journals' }
	| { type: 'fork'; block_id: string; session_id?: string }
	| { type: 'rerun'; block_id: string; session_id?: string }
	| { type: 'cancel'; block_id: string }
	| { type: 'subscribe'; journal_id: string; name: string; kind?: string }
	| { type: 'unsubscribe'; journal_id: string }
	| { type: 'cursor'; journal_id: string; block_id?: string; offset?: number }
	| { type: 'get_presence'; journal_id: string }
	| { type: 'crdt_update'; journal_id: string; update: string }
	| { type: 'sync_request'; journal_id: string; state_vector?: string }
	| {
			type: 'register_participant';
			journal_id: string;
			name: string;
			kind?: string;
			capabilities?: string[];
	  }
	| {
			type: 'delegate';
			journal_id: string;
			description: string;
			assignee_id: string;
			block_id?: string;
			priority?: string;
			requires_approval?: boolean;
			approver_id?: string;
	  }
	| { type: 'accept_work'; work_item_id: string }
	| { type: 'decline_work'; work_item_id: string }
	| { type: 'submit_work'; work_item_id: string; result: string }
	| { type: 'approve_work'; approval_id: string; feedback?: string }
	| { type: 'reject_work'; approval_id: string; feedback: string }
	| { type: 'cancel_work'; work_item_id: string }
	| { type: 'claim_work'; work_item_id: string }
	| { type: 'get_work_queue' }
	| { type: 'get_approval_queue' }
	| { type: 'set_accepting_work'; accepting: boolean }
	| { type: 'get_participants'; journal_id: string };

// Server -> Client messages
export type ServerMessage =
	| { type: 'journal_created'; journal_id: string; title: string }
	| { type: 'journal'; journal: Journal; blocks: Block[] }
	| { type: 'journals'; journals: Journal[] }
	| { type: 'block_created'; block: Block }
	| { type: 'block_content_delta'; block_id: string; delta: string }
	| { type: 'block_status_changed'; block_id: string; status: Block['status'] }
	| { type: 'block_forked'; original_block_id: string; new_block: Block }
	| { type: 'block_cancelled'; block_id: string }
	| { type: 'error'; message: string; details?: string }
	| {
			type: 'subscribed';
			journal_id: string;
			participant: Participant;
			participants: Participant[];
	  }
	| { type: 'unsubscribed'; journal_id: string }
	| { type: 'participant_joined'; journal_id: string; participant: Participant }
	| { type: 'participant_left'; journal_id: string; participant_id: string }
	| {
			type: 'cursor_moved';
			journal_id: string;
			participant_id: string;
			block_id?: string;
			offset?: number;
	  }
	| {
			type: 'participant_status_changed';
			journal_id: string;
			participant_id: string;
			status: Participant['status'];
	  }
	| { type: 'presence'; journal_id: string; participants: Participant[] }
	| { type: 'crdt_update'; journal_id: string; source?: string; update: string }
	| { type: 'sync_state'; journal_id: string; state: string }
	| {
			type: 'participant_registered';
			participant_id: string;
			name: string;
			kind: string;
			capabilities: string[];
	  }
	| { type: 'work_delegated'; work_item: WorkItem }
	| { type: 'work_accepted'; work_item_id: string; assignee_id: string }
	| { type: 'work_declined'; work_item_id: string; assignee_id: string }
	| { type: 'approval_requested'; approval: ApprovalRequest; work_item: WorkItem }
	| { type: 'work_approved'; work_item_id: string; approver_id: string; feedback?: string }
	| { type: 'work_rejected'; work_item_id: string; approver_id: string; feedback: string }
	| { type: 'work_cancelled'; work_item_id: string; cancelled_by: string }
	| { type: 'work_claimed'; work_item_id: string; claimed_by: string }
	| { type: 'work_queue'; items: WorkItem[] }
	| { type: 'approval_queue'; items: ApprovalRequest[] }
	| {
			type: 'available_participants';
			participants: Array<{ id: string; name: string; kind: string; capabilities: string[] }>;
	  }
	| { type: 'accepting_work_changed'; participant_id: string; accepting: boolean };
