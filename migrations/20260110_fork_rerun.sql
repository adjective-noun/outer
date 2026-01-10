-- Add fork and rerun support to blocks table

-- Parent block ID for timeline branching (the block this was forked after)
ALTER TABLE blocks ADD COLUMN parent_id TEXT REFERENCES blocks(id);

-- Original block ID that was forked/re-run to create this block
ALTER TABLE blocks ADD COLUMN forked_from_id TEXT REFERENCES blocks(id);

-- Index for efficient timeline queries
CREATE INDEX IF NOT EXISTS idx_blocks_parent_id ON blocks(parent_id);
CREATE INDEX IF NOT EXISTS idx_blocks_forked_from_id ON blocks(forked_from_id);
