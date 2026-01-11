-- Initial schema for outer.sh

-- Journals table
CREATE TABLE IF NOT EXISTS journals (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Blocks table
CREATE TABLE IF NOT EXISTS blocks (
    id TEXT PRIMARY KEY NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    block_type TEXT NOT NULL CHECK (block_type IN ('user', 'assistant')),
    content TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'streaming', 'complete', 'error')),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_blocks_journal_id ON blocks(journal_id);
CREATE INDEX IF NOT EXISTS idx_blocks_created_at ON blocks(created_at);
CREATE INDEX IF NOT EXISTS idx_journals_updated_at ON journals(updated_at);
