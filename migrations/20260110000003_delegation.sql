-- Delegation and approval system tables

-- Registered participants with capabilities
CREATE TABLE IF NOT EXISTS registered_participants (
    id TEXT PRIMARY KEY NOT NULL,
    participant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('user', 'agent', 'observer')),
    capabilities TEXT NOT NULL DEFAULT '[]', -- JSON array of capabilities
    accepting_work BOOLEAN NOT NULL DEFAULT 1,
    work_capacity INTEGER NOT NULL DEFAULT 5,
    registered_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Work items (delegated tasks)
CREATE TABLE IF NOT EXISTS work_items (
    id TEXT PRIMARY KEY NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    description TEXT NOT NULL,
    block_id TEXT REFERENCES blocks(id),
    delegator_id TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending', 'in_progress', 'awaiting_approval',
        'approved', 'rejected', 'declined', 'cancelled'
    )),
    priority TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    requires_approval BOOLEAN NOT NULL DEFAULT 0,
    approver_id TEXT,
    result TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Approval requests
CREATE TABLE IF NOT EXISTS approval_requests (
    id TEXT PRIMARY KEY NOT NULL,
    work_item_id TEXT NOT NULL REFERENCES work_items(id),
    requester_id TEXT NOT NULL,
    approver_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    feedback TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at DATETIME
);

-- Indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_registered_participants_participant_id ON registered_participants(participant_id);
CREATE INDEX IF NOT EXISTS idx_work_items_journal_id ON work_items(journal_id);
CREATE INDEX IF NOT EXISTS idx_work_items_assignee_id ON work_items(assignee_id);
CREATE INDEX IF NOT EXISTS idx_work_items_delegator_id ON work_items(delegator_id);
CREATE INDEX IF NOT EXISTS idx_work_items_status ON work_items(status);
CREATE INDEX IF NOT EXISTS idx_approval_requests_work_item_id ON approval_requests(work_item_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_approver_id ON approval_requests(approver_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
