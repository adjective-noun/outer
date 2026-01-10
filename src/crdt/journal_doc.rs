//! CRDT document wrapper for journals
//!
//! Wraps a Yrs document to provide collaborative editing capabilities
//! for journal blocks.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, Map, MapRef, ReadTxn, Text, TextRef, Transact, Update};

/// CRDT document for a journal
///
/// Contains:
/// - A map of block IDs to their content (as CRDT text)
/// - Metadata about the journal
pub struct JournalDoc {
    doc: Doc,
    journal_id: Uuid,
}

impl JournalDoc {
    /// Create a new empty journal document
    pub fn new(journal_id: Uuid) -> Self {
        Self {
            doc: Doc::new(),
            journal_id,
        }
    }

    /// Create from an existing state vector (for syncing)
    pub fn from_update(journal_id: Uuid, update: &[u8]) -> Result<Self, yrs::encoding::read::Error> {
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let update_decoded = Update::decode_v1(update)?;
            txn.apply_update(update_decoded);
        }
        Ok(Self { doc, journal_id })
    }

    /// Get the journal ID
    pub fn journal_id(&self) -> Uuid {
        self.journal_id
    }

    /// Get the underlying Yrs document
    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    /// Get the blocks map
    fn blocks_map(&self) -> MapRef {
        self.doc.get_or_insert_map("blocks")
    }

    /// Get or create a text entry for a block
    pub fn get_or_create_block_text(&self, block_id: Uuid) -> TextRef {
        let map = self.blocks_map();
        let key = block_id.to_string();

        {
            let txn = self.doc.transact();
            if let Some(yrs::Value::YText(text)) = map.get(&txn, &key) {
                return text;
            }
        }

        let mut txn = self.doc.transact_mut();
        // Insert a new empty text into the map
        map.insert(&mut txn, key.clone(), yrs::TextPrelim::new(""));
        // Now retrieve it
        if let Some(yrs::Value::YText(text)) = map.get(&txn, &key) {
            return text;
        }
        // This shouldn't happen, but fallback to creating a root-level text
        self.doc.get_or_insert_text(key.as_str())
    }

    /// Get a block's content
    pub fn get_block_content(&self, block_id: Uuid) -> Option<String> {
        let map = self.blocks_map();
        let txn = self.doc.transact();
        let key = block_id.to_string();

        match map.get(&txn, &key) {
            Some(yrs::Value::YText(text)) => Some(text.get_string(&txn)),
            _ => None,
        }
    }

    /// Set a block's content (replaces existing content)
    pub fn set_block_content(&self, block_id: Uuid, content: &str) {
        let text = self.get_or_create_block_text(block_id);
        let mut txn = self.doc.transact_mut();
        let len = text.len(&txn);
        if len > 0 {
            text.remove_range(&mut txn, 0, len);
        }
        text.insert(&mut txn, 0, content);
    }

    /// Append to a block's content (for streaming)
    pub fn append_block_content(&self, block_id: Uuid, delta: &str) {
        let text = self.get_or_create_block_text(block_id);
        let mut txn = self.doc.transact_mut();
        let len = text.len(&txn);
        text.insert(&mut txn, len, delta);
    }

    /// Delete a block
    pub fn delete_block(&self, block_id: Uuid) {
        let map = self.blocks_map();
        let mut txn = self.doc.transact_mut();
        map.remove(&mut txn, &block_id.to_string());
    }

    /// Get the full state as a binary update
    pub fn encode_state(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&Default::default())
    }

    /// Get the state vector (for sync protocol)
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// Compute update from a remote state vector
    pub fn encode_diff(&self, remote_sv: &[u8]) -> Result<Vec<u8>, yrs::encoding::read::Error> {
        let sv = yrs::StateVector::decode_v1(remote_sv)?;
        let txn = self.doc.transact();
        Ok(txn.encode_state_as_update_v1(&sv))
    }

    /// Apply a remote update
    pub fn apply_update(&self, update: &[u8]) -> Result<(), yrs::encoding::read::Error> {
        let mut txn = self.doc.transact_mut();
        let update_decoded = Update::decode_v1(update)?;
        txn.apply_update(update_decoded);
        Ok(())
    }

    /// List all block IDs in the document
    pub fn list_blocks(&self) -> Vec<Uuid> {
        let map = self.blocks_map();
        let txn = self.doc.transact();
        map.keys(&txn)
            .filter_map(|k| Uuid::parse_str(k).ok())
            .collect()
    }
}

/// Manager for multiple journal documents
pub struct JournalDocManager {
    docs: RwLock<HashMap<Uuid, Arc<JournalDoc>>>,
}

impl JournalDocManager {
    pub fn new() -> Self {
        Self {
            docs: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a journal document
    pub async fn get_or_create(&self, journal_id: Uuid) -> Arc<JournalDoc> {
        {
            let docs = self.docs.read().await;
            if let Some(doc) = docs.get(&journal_id) {
                return Arc::clone(doc);
            }
        }

        let mut docs = self.docs.write().await;
        // Double-check after acquiring write lock
        if let Some(doc) = docs.get(&journal_id) {
            return Arc::clone(doc);
        }

        let doc = Arc::new(JournalDoc::new(journal_id));
        docs.insert(journal_id, Arc::clone(&doc));
        doc
    }

    /// Remove a journal document from the manager
    pub async fn remove(&self, journal_id: Uuid) -> Option<Arc<JournalDoc>> {
        let mut docs = self.docs.write().await;
        docs.remove(&journal_id)
    }

    /// Check if a journal document exists
    pub async fn contains(&self, journal_id: Uuid) -> bool {
        let docs = self.docs.read().await;
        docs.contains_key(&journal_id)
    }
}

impl Default for JournalDocManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_journal_doc() {
        let journal_id = Uuid::new_v4();
        let doc = JournalDoc::new(journal_id);
        assert_eq!(doc.journal_id(), journal_id);
    }

    #[test]
    fn test_set_and_get_block_content() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        doc.set_block_content(block_id, "Hello, world!");
        assert_eq!(doc.get_block_content(block_id), Some("Hello, world!".to_string()));
    }

    #[test]
    fn test_append_block_content() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        doc.set_block_content(block_id, "Hello");
        doc.append_block_content(block_id, ", world!");
        assert_eq!(doc.get_block_content(block_id), Some("Hello, world!".to_string()));
    }

    #[test]
    fn test_delete_block() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        doc.set_block_content(block_id, "Content");
        assert!(doc.get_block_content(block_id).is_some());

        doc.delete_block(block_id);
        assert!(doc.get_block_content(block_id).is_none());
    }

    #[test]
    fn test_list_blocks() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block1 = Uuid::new_v4();
        let block2 = Uuid::new_v4();

        doc.set_block_content(block1, "Content 1");
        doc.set_block_content(block2, "Content 2");

        let blocks = doc.list_blocks();
        assert_eq!(blocks.len(), 2);
        assert!(blocks.contains(&block1));
        assert!(blocks.contains(&block2));
    }

    #[test]
    fn test_encode_and_apply_update() {
        let doc1 = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();
        doc1.set_block_content(block_id, "Hello from doc1");

        let update = doc1.encode_state();

        let doc2 = JournalDoc::from_update(doc1.journal_id(), &update).unwrap();
        assert_eq!(doc2.get_block_content(block_id), Some("Hello from doc1".to_string()));
    }

    #[test]
    fn test_encode_diff() {
        let doc1 = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();
        doc1.set_block_content(block_id, "Initial");

        // Simulate a remote doc with no state
        let empty_sv = yrs::StateVector::default().encode_v1();
        let diff = doc1.encode_diff(&empty_sv).unwrap();

        // Apply diff to new doc
        let doc2 = JournalDoc::new(doc1.journal_id());
        doc2.apply_update(&diff).unwrap();
        assert_eq!(doc2.get_block_content(block_id), Some("Initial".to_string()));
    }

    #[test]
    fn test_concurrent_edits_merge() {
        let journal_id = Uuid::new_v4();
        let block_id = Uuid::new_v4();

        // Two docs with same initial state
        let doc1 = JournalDoc::new(journal_id);
        let doc2 = JournalDoc::new(journal_id);

        // Each makes independent edits
        doc1.set_block_content(block_id, "Hello");
        doc2.set_block_content(block_id, "World");

        // Get updates
        let update1 = doc1.encode_state();
        let update2 = doc2.encode_state();

        // Apply updates to a third doc
        let doc3 = JournalDoc::new(journal_id);
        doc3.apply_update(&update1).unwrap();
        doc3.apply_update(&update2).unwrap();

        // Content should merge (CRDT guarantees)
        let content = doc3.get_block_content(block_id);
        assert!(content.is_some());
        // The exact merge result depends on Yrs implementation
    }

    #[tokio::test]
    async fn test_journal_doc_manager_get_or_create() {
        let manager = JournalDocManager::new();
        let journal_id = Uuid::new_v4();

        let doc1 = manager.get_or_create(journal_id).await;
        let doc2 = manager.get_or_create(journal_id).await;

        // Should return the same Arc
        assert!(Arc::ptr_eq(&doc1, &doc2));
    }

    #[tokio::test]
    async fn test_journal_doc_manager_remove() {
        let manager = JournalDocManager::new();
        let journal_id = Uuid::new_v4();

        manager.get_or_create(journal_id).await;
        assert!(manager.contains(journal_id).await);

        manager.remove(journal_id).await;
        assert!(!manager.contains(journal_id).await);
    }

    #[tokio::test]
    async fn test_journal_doc_manager_different_journals() {
        let manager = JournalDocManager::new();
        let journal1 = Uuid::new_v4();
        let journal2 = Uuid::new_v4();

        let doc1 = manager.get_or_create(journal1).await;
        let doc2 = manager.get_or_create(journal2).await;

        assert!(!Arc::ptr_eq(&doc1, &doc2));
        assert_eq!(doc1.journal_id(), journal1);
        assert_eq!(doc2.journal_id(), journal2);
    }

    #[test]
    fn test_get_block_content_nonexistent() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        assert!(doc.get_block_content(block_id).is_none());
    }

    #[test]
    fn test_state_vector() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let sv = doc.state_vector();
        assert!(!sv.is_empty());
    }

    #[test]
    fn test_set_block_content_overwrites() {
        let doc = JournalDoc::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        doc.set_block_content(block_id, "First");
        doc.set_block_content(block_id, "Second");

        assert_eq!(doc.get_block_content(block_id), Some("Second".to_string()));
    }
}
