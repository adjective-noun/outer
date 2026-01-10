//! CRDT module for real-time collaborative editing
//!
//! Uses Yrs (Yjs port) for conflict-free replicated data types.

pub mod journal_doc;
pub mod participant;
pub mod room;

pub use journal_doc::JournalDoc;
pub use participant::{Participant, ParticipantKind, ParticipantStatus};
pub use room::{JournalRoom, RoomEvent};
