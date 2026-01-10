//! Journal room for managing subscribers and real-time sync
//!
//! A room represents all clients subscribed to a particular journal,
//! managing CRDT updates and presence information.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use super::journal_doc::JournalDoc;
use super::participant::{Participant, ParticipantKind};

/// Events that can occur in a journal room
#[derive(Debug, Clone)]
pub enum RoomEvent {
    /// A participant joined the room
    ParticipantJoined(Participant),
    /// A participant left the room
    ParticipantLeft { participant_id: Uuid },
    /// A participant's cursor position changed
    CursorMoved {
        participant_id: Uuid,
        block_id: Option<Uuid>,
        offset: Option<u32>,
    },
    /// A participant's status changed (active/idle/disconnected)
    StatusChanged {
        participant_id: Uuid,
        status: super::participant::ParticipantStatus,
    },
    /// CRDT update received (binary update to apply)
    CrdtUpdate {
        /// The participant who made the change (None for server-originated)
        source: Option<Uuid>,
        /// The binary update data
        update: Vec<u8>,
    },
    /// Full sync requested/sent
    SyncState {
        /// Full document state
        state: Vec<u8>,
    },
}

/// A room for a journal, managing subscribers and CRDT sync
pub struct JournalRoom {
    journal_id: Uuid,
    doc: Arc<JournalDoc>,
    participants: RwLock<HashMap<Uuid, Participant>>,
    event_tx: broadcast::Sender<RoomEvent>,
}

impl JournalRoom {
    /// Create a new room for a journal
    pub fn new(journal_id: Uuid) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            journal_id,
            doc: Arc::new(JournalDoc::new(journal_id)),
            participants: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Create a room with an existing CRDT document
    pub fn with_doc(doc: Arc<JournalDoc>) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            journal_id: doc.journal_id(),
            doc,
            participants: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Get the journal ID
    pub fn journal_id(&self) -> Uuid {
        self.journal_id
    }

    /// Get the CRDT document
    pub fn doc(&self) -> &Arc<JournalDoc> {
        &self.doc
    }

    /// Subscribe to room events
    pub fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.event_tx.subscribe()
    }

    /// Add a participant to the room
    pub async fn join(&self, name: impl Into<String>, kind: ParticipantKind) -> Participant {
        let participant = Participant::new(name, kind);
        let mut participants = self.participants.write().await;
        participants.insert(participant.id, participant.clone());

        // Broadcast join event
        let _ = self.event_tx.send(RoomEvent::ParticipantJoined(participant.clone()));

        participant
    }

    /// Add a participant with a specific ID (for reconnection)
    pub async fn rejoin(&self, participant: Participant) -> Participant {
        let mut participants = self.participants.write().await;
        participants.insert(participant.id, participant.clone());

        let _ = self.event_tx.send(RoomEvent::ParticipantJoined(participant.clone()));

        participant
    }

    /// Remove a participant from the room
    pub async fn leave(&self, participant_id: Uuid) -> Option<Participant> {
        let mut participants = self.participants.write().await;
        let removed = participants.remove(&participant_id);

        if removed.is_some() {
            let _ = self.event_tx.send(RoomEvent::ParticipantLeft { participant_id });
        }

        removed
    }

    /// Update a participant's cursor position
    pub async fn update_cursor(
        &self,
        participant_id: Uuid,
        block_id: Option<Uuid>,
        offset: Option<u32>,
    ) -> bool {
        let mut participants = self.participants.write().await;
        if let Some(participant) = participants.get_mut(&participant_id) {
            participant.set_cursor(block_id, offset);

            let _ = self.event_tx.send(RoomEvent::CursorMoved {
                participant_id,
                block_id,
                offset,
            });

            true
        } else {
            false
        }
    }

    /// Get all current participants
    pub async fn participants(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants.values().cloned().collect()
    }

    /// Get a specific participant
    pub async fn get_participant(&self, participant_id: Uuid) -> Option<Participant> {
        let participants = self.participants.read().await;
        participants.get(&participant_id).cloned()
    }

    /// Get the number of participants
    pub async fn participant_count(&self) -> usize {
        let participants = self.participants.read().await;
        participants.len()
    }

    /// Check if the room is empty
    pub async fn is_empty(&self) -> bool {
        let participants = self.participants.read().await;
        participants.is_empty()
    }

    /// Apply a CRDT update from a participant
    pub async fn apply_update(&self, source: Option<Uuid>, update: &[u8]) -> Result<(), yrs::encoding::read::Error> {
        self.doc.apply_update(update)?;

        // Broadcast to all other participants
        let _ = self.event_tx.send(RoomEvent::CrdtUpdate {
            source,
            update: update.to_vec(),
        });

        Ok(())
    }

    /// Get the full sync state for a new participant
    pub fn get_sync_state(&self) -> Vec<u8> {
        self.doc.encode_state()
    }

    /// Broadcast the full sync state
    pub fn broadcast_sync(&self) {
        let state = self.doc.encode_state();
        let _ = self.event_tx.send(RoomEvent::SyncState { state });
    }

    /// Set content for a block and broadcast the update
    pub async fn set_block_content(&self, block_id: Uuid, content: &str, source: Option<Uuid>) {
        // Get state before
        let before_sv = self.doc.state_vector();

        // Apply the change
        self.doc.set_block_content(block_id, content);

        // Compute the update (diff from before)
        if let Ok(update) = self.doc.encode_diff(&before_sv) {
            let _ = self.event_tx.send(RoomEvent::CrdtUpdate {
                source,
                update,
            });
        }
    }

    /// Append content to a block and broadcast the update
    pub async fn append_block_content(&self, block_id: Uuid, delta: &str, source: Option<Uuid>) {
        let before_sv = self.doc.state_vector();

        self.doc.append_block_content(block_id, delta);

        if let Ok(update) = self.doc.encode_diff(&before_sv) {
            let _ = self.event_tx.send(RoomEvent::CrdtUpdate {
                source,
                update,
            });
        }
    }

    /// Mark stale participants as idle/disconnected
    pub async fn cleanup_stale_participants(&self, idle_timeout: chrono::Duration, disconnect_timeout: chrono::Duration) {
        let mut participants = self.participants.write().await;

        for participant in participants.values_mut() {
            if participant.is_stale(disconnect_timeout) && participant.status != super::participant::ParticipantStatus::Disconnected {
                participant.mark_disconnected();
                let _ = self.event_tx.send(RoomEvent::StatusChanged {
                    participant_id: participant.id,
                    status: participant.status,
                });
            } else if participant.is_stale(idle_timeout) && participant.status == super::participant::ParticipantStatus::Active {
                participant.mark_idle();
                let _ = self.event_tx.send(RoomEvent::StatusChanged {
                    participant_id: participant.id,
                    status: participant.status,
                });
            }
        }
    }
}

/// Manager for all active journal rooms
pub struct RoomManager {
    rooms: RwLock<HashMap<Uuid, Arc<JournalRoom>>>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a room for a journal
    pub async fn get_or_create(&self, journal_id: Uuid) -> Arc<JournalRoom> {
        {
            let rooms = self.rooms.read().await;
            if let Some(room) = rooms.get(&journal_id) {
                return Arc::clone(room);
            }
        }

        let mut rooms = self.rooms.write().await;
        // Double-check after acquiring write lock
        if let Some(room) = rooms.get(&journal_id) {
            return Arc::clone(room);
        }

        let room = Arc::new(JournalRoom::new(journal_id));
        rooms.insert(journal_id, Arc::clone(&room));
        room
    }

    /// Get a room if it exists
    pub async fn get(&self, journal_id: Uuid) -> Option<Arc<JournalRoom>> {
        let rooms = self.rooms.read().await;
        rooms.get(&journal_id).cloned()
    }

    /// Remove a room (usually when empty)
    pub async fn remove(&self, journal_id: Uuid) -> Option<Arc<JournalRoom>> {
        let mut rooms = self.rooms.write().await;
        rooms.remove(&journal_id)
    }

    /// Remove empty rooms
    pub async fn cleanup_empty_rooms(&self) {
        let mut rooms = self.rooms.write().await;
        let empty_journals: Vec<Uuid> = {
            let mut empties = Vec::new();
            for (journal_id, room) in rooms.iter() {
                if room.is_empty().await {
                    empties.push(*journal_id);
                }
            }
            empties
        };

        for journal_id in empty_journals {
            rooms.remove(&journal_id);
        }
    }

    /// Get the number of active rooms
    pub async fn room_count(&self) -> usize {
        let rooms = self.rooms.read().await;
        rooms.len()
    }
}

impl Default for RoomManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_room_join_and_leave() {
        let room = JournalRoom::new(Uuid::new_v4());

        let participant = room.join("Alice", ParticipantKind::User).await;
        assert_eq!(room.participant_count().await, 1);

        room.leave(participant.id).await;
        assert_eq!(room.participant_count().await, 0);
    }

    #[tokio::test]
    async fn test_room_multiple_participants() {
        let room = JournalRoom::new(Uuid::new_v4());

        room.join("Alice", ParticipantKind::User).await;
        room.join("Bob", ParticipantKind::User).await;
        room.join("Agent", ParticipantKind::Agent).await;

        assert_eq!(room.participant_count().await, 3);

        let participants = room.participants().await;
        assert_eq!(participants.len(), 3);
    }

    #[tokio::test]
    async fn test_room_update_cursor() {
        let room = JournalRoom::new(Uuid::new_v4());
        let participant = room.join("Alice", ParticipantKind::User).await;
        let block_id = Uuid::new_v4();

        let updated = room.update_cursor(participant.id, Some(block_id), Some(42)).await;
        assert!(updated);

        let p = room.get_participant(participant.id).await.unwrap();
        assert_eq!(p.cursor_block_id, Some(block_id));
        assert_eq!(p.cursor_offset, Some(42));
    }

    #[tokio::test]
    async fn test_room_update_cursor_invalid_participant() {
        let room = JournalRoom::new(Uuid::new_v4());
        let updated = room.update_cursor(Uuid::new_v4(), None, None).await;
        assert!(!updated);
    }

    #[tokio::test]
    async fn test_room_event_subscription() {
        let room = JournalRoom::new(Uuid::new_v4());
        let mut receiver = room.subscribe();

        room.join("Alice", ParticipantKind::User).await;

        let event = receiver.try_recv().unwrap();
        match event {
            RoomEvent::ParticipantJoined(p) => {
                assert_eq!(p.name, "Alice");
            }
            _ => panic!("Expected ParticipantJoined event"),
        }
    }

    #[tokio::test]
    async fn test_room_crdt_operations() {
        let room = JournalRoom::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        room.set_block_content(block_id, "Hello", None).await;
        assert_eq!(room.doc().get_block_content(block_id), Some("Hello".to_string()));

        room.append_block_content(block_id, " World", None).await;
        assert_eq!(room.doc().get_block_content(block_id), Some("Hello World".to_string()));
    }

    #[tokio::test]
    async fn test_room_sync_state() {
        let room = JournalRoom::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        room.set_block_content(block_id, "Test content", None).await;

        let state = room.get_sync_state();
        assert!(!state.is_empty());
    }

    #[tokio::test]
    async fn test_room_manager_get_or_create() {
        let manager = RoomManager::new();
        let journal_id = Uuid::new_v4();

        let room1 = manager.get_or_create(journal_id).await;
        let room2 = manager.get_or_create(journal_id).await;

        assert!(Arc::ptr_eq(&room1, &room2));
    }

    #[tokio::test]
    async fn test_room_manager_different_journals() {
        let manager = RoomManager::new();
        let j1 = Uuid::new_v4();
        let j2 = Uuid::new_v4();

        let room1 = manager.get_or_create(j1).await;
        let room2 = manager.get_or_create(j2).await;

        assert!(!Arc::ptr_eq(&room1, &room2));
        assert_eq!(manager.room_count().await, 2);
    }

    #[tokio::test]
    async fn test_room_manager_get_nonexistent() {
        let manager = RoomManager::new();
        let room = manager.get(Uuid::new_v4()).await;
        assert!(room.is_none());
    }

    #[tokio::test]
    async fn test_room_manager_remove() {
        let manager = RoomManager::new();
        let journal_id = Uuid::new_v4();

        manager.get_or_create(journal_id).await;
        assert_eq!(manager.room_count().await, 1);

        manager.remove(journal_id).await;
        assert_eq!(manager.room_count().await, 0);
    }

    #[tokio::test]
    async fn test_room_manager_cleanup_empty() {
        let manager = RoomManager::new();
        let journal_id = Uuid::new_v4();

        let room = manager.get_or_create(journal_id).await;
        let participant = room.join("Alice", ParticipantKind::User).await;

        // Room has participant, shouldn't be cleaned up
        manager.cleanup_empty_rooms().await;
        assert_eq!(manager.room_count().await, 1);

        // Remove participant
        room.leave(participant.id).await;

        // Now cleanup should remove the room
        manager.cleanup_empty_rooms().await;
        assert_eq!(manager.room_count().await, 0);
    }

    #[tokio::test]
    async fn test_room_is_empty() {
        let room = JournalRoom::new(Uuid::new_v4());
        assert!(room.is_empty().await);

        let participant = room.join("Alice", ParticipantKind::User).await;
        assert!(!room.is_empty().await);

        room.leave(participant.id).await;
        assert!(room.is_empty().await);
    }

    #[tokio::test]
    async fn test_room_rejoin() {
        let room = JournalRoom::new(Uuid::new_v4());
        let original = room.join("Alice", ParticipantKind::User).await;
        let original_id = original.id;

        room.leave(original_id).await;
        assert!(room.is_empty().await);

        // Rejoin with same ID
        let participant = Participant::with_id(original_id, "Alice", ParticipantKind::User);
        room.rejoin(participant).await;

        let p = room.get_participant(original_id).await;
        assert!(p.is_some());
        assert_eq!(p.unwrap().id, original_id);
    }

    #[tokio::test]
    async fn test_room_with_doc() {
        let journal_id = Uuid::new_v4();
        let doc = Arc::new(JournalDoc::new(journal_id));
        let block_id = Uuid::new_v4();

        doc.set_block_content(block_id, "Pre-existing content");

        let room = JournalRoom::with_doc(doc);

        assert_eq!(room.journal_id(), journal_id);
        assert_eq!(room.doc().get_block_content(block_id), Some("Pre-existing content".to_string()));
    }

    #[tokio::test]
    async fn test_room_apply_update() {
        let room1 = JournalRoom::new(Uuid::new_v4());
        let room2 = JournalRoom::new(Uuid::new_v4());
        let block_id = Uuid::new_v4();

        room1.set_block_content(block_id, "Shared content", None).await;
        let update = room1.doc().encode_state();

        room2.apply_update(None, &update).await.unwrap();

        assert_eq!(room2.doc().get_block_content(block_id), Some("Shared content".to_string()));
    }

    #[tokio::test]
    async fn test_room_leave_event() {
        let room = JournalRoom::new(Uuid::new_v4());
        let participant = room.join("Alice", ParticipantKind::User).await;
        let participant_id = participant.id;

        let mut receiver = room.subscribe();

        room.leave(participant_id).await;

        // Skip any prior events and find the leave event
        loop {
            match receiver.try_recv() {
                Ok(RoomEvent::ParticipantLeft { participant_id: id }) => {
                    assert_eq!(id, participant_id);
                    break;
                }
                Ok(_) => continue,
                Err(_) => panic!("Expected ParticipantLeft event"),
            }
        }
    }

    #[tokio::test]
    async fn test_room_cursor_moved_event() {
        let room = JournalRoom::new(Uuid::new_v4());
        let participant = room.join("Alice", ParticipantKind::User).await;
        let mut receiver = room.subscribe();

        // Clear the join event
        let _ = receiver.try_recv();

        let block_id = Uuid::new_v4();
        room.update_cursor(participant.id, Some(block_id), Some(10)).await;

        let event = receiver.try_recv().unwrap();
        match event {
            RoomEvent::CursorMoved { participant_id, block_id: bid, offset } => {
                assert_eq!(participant_id, participant.id);
                assert_eq!(bid, Some(block_id));
                assert_eq!(offset, Some(10));
            }
            _ => panic!("Expected CursorMoved event"),
        }
    }
}
