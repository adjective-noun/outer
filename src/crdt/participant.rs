//! Participant model for presence tracking
//!
//! Tracks users and agents viewing/editing a journal.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of participant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    /// Human user
    User,
    /// AI agent (e.g., OpenCode session)
    Agent,
    /// System observer (e.g., monitoring)
    Observer,
}

impl ParticipantKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParticipantKind::User => "user",
            ParticipantKind::Agent => "agent",
            ParticipantKind::Observer => "observer",
        }
    }
}

impl std::str::FromStr for ParticipantKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(ParticipantKind::User),
            "agent" => Ok(ParticipantKind::Agent),
            "observer" => Ok(ParticipantKind::Observer),
            _ => Err(format!("Invalid participant kind: {}", s)),
        }
    }
}

/// Participant connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    /// Actively connected and responsive
    Active,
    /// Connected but idle
    Idle,
    /// Connection lost, may reconnect
    Disconnected,
}

impl ParticipantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParticipantStatus::Active => "active",
            ParticipantStatus::Idle => "idle",
            ParticipantStatus::Disconnected => "disconnected",
        }
    }
}

impl std::str::FromStr for ParticipantStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(ParticipantStatus::Active),
            "idle" => Ok(ParticipantStatus::Idle),
            "disconnected" => Ok(ParticipantStatus::Disconnected),
            _ => Err(format!("Invalid participant status: {}", s)),
        }
    }
}

/// Predefined colors for participant cursors
const PARTICIPANT_COLORS: [&str; 8] = [
    "#FF6B6B", // Red
    "#4ECDC4", // Teal
    "#45B7D1", // Blue
    "#96CEB4", // Green
    "#FFEAA7", // Yellow
    "#DDA0DD", // Plum
    "#98D8C8", // Mint
    "#F7DC6F", // Gold
];

/// A participant in a journal room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Unique identifier for this participant session
    pub id: Uuid,
    /// Display name
    pub name: String,
    /// Type of participant
    pub kind: ParticipantKind,
    /// Current connection status
    pub status: ParticipantStatus,
    /// Block ID where the cursor is positioned (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_block_id: Option<Uuid>,
    /// Character offset within the block (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_offset: Option<u32>,
    /// Color for displaying this participant's cursor/presence
    pub color: String,
    /// When this participant joined
    pub joined_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_seen_at: DateTime<Utc>,
}

impl Participant {
    /// Create a new participant
    pub fn new(name: impl Into<String>, kind: ParticipantKind) -> Self {
        let id = Uuid::new_v4();
        let now = Utc::now();

        // Pick a color based on the UUID (deterministic but distributed)
        let color_index = (id.as_bytes()[0] as usize) % PARTICIPANT_COLORS.len();

        Self {
            id,
            name: name.into(),
            kind,
            status: ParticipantStatus::Active,
            cursor_block_id: None,
            cursor_offset: None,
            color: PARTICIPANT_COLORS[color_index].to_string(),
            joined_at: now,
            last_seen_at: now,
        }
    }

    /// Create with a specific ID (useful for reconnection)
    pub fn with_id(id: Uuid, name: impl Into<String>, kind: ParticipantKind) -> Self {
        let now = Utc::now();
        let color_index = (id.as_bytes()[0] as usize) % PARTICIPANT_COLORS.len();

        Self {
            id,
            name: name.into(),
            kind,
            status: ParticipantStatus::Active,
            cursor_block_id: None,
            cursor_offset: None,
            color: PARTICIPANT_COLORS[color_index].to_string(),
            joined_at: now,
            last_seen_at: now,
        }
    }

    /// Update cursor position
    pub fn set_cursor(&mut self, block_id: Option<Uuid>, offset: Option<u32>) {
        self.cursor_block_id = block_id;
        self.cursor_offset = offset;
        self.touch();
    }

    /// Update last seen timestamp
    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
        if self.status == ParticipantStatus::Idle {
            self.status = ParticipantStatus::Active;
        }
    }

    /// Mark as idle
    pub fn mark_idle(&mut self) {
        self.status = ParticipantStatus::Idle;
    }

    /// Mark as disconnected
    pub fn mark_disconnected(&mut self) {
        self.status = ParticipantStatus::Disconnected;
    }

    /// Check if participant has been inactive for a duration
    pub fn is_stale(&self, timeout: chrono::Duration) -> bool {
        Utc::now().signed_duration_since(self.last_seen_at) > timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_new() {
        let p = Participant::new("Alice", ParticipantKind::User);
        assert_eq!(p.name, "Alice");
        assert_eq!(p.kind, ParticipantKind::User);
        assert_eq!(p.status, ParticipantStatus::Active);
        assert!(p.cursor_block_id.is_none());
        assert!(!p.color.is_empty());
    }

    #[test]
    fn test_participant_with_id() {
        let id = Uuid::new_v4();
        let p = Participant::with_id(id, "Bob", ParticipantKind::Agent);
        assert_eq!(p.id, id);
        assert_eq!(p.name, "Bob");
        assert_eq!(p.kind, ParticipantKind::Agent);
    }

    #[test]
    fn test_set_cursor() {
        let mut p = Participant::new("Test", ParticipantKind::User);
        let block_id = Uuid::new_v4();

        p.set_cursor(Some(block_id), Some(42));

        assert_eq!(p.cursor_block_id, Some(block_id));
        assert_eq!(p.cursor_offset, Some(42));
    }

    #[test]
    fn test_mark_idle() {
        let mut p = Participant::new("Test", ParticipantKind::User);
        assert_eq!(p.status, ParticipantStatus::Active);

        p.mark_idle();
        assert_eq!(p.status, ParticipantStatus::Idle);
    }

    #[test]
    fn test_mark_disconnected() {
        let mut p = Participant::new("Test", ParticipantKind::User);
        p.mark_disconnected();
        assert_eq!(p.status, ParticipantStatus::Disconnected);
    }

    #[test]
    fn test_touch_reactivates_idle() {
        let mut p = Participant::new("Test", ParticipantKind::User);
        p.mark_idle();
        assert_eq!(p.status, ParticipantStatus::Idle);

        p.touch();
        assert_eq!(p.status, ParticipantStatus::Active);
    }

    #[test]
    fn test_participant_kind_as_str() {
        assert_eq!(ParticipantKind::User.as_str(), "user");
        assert_eq!(ParticipantKind::Agent.as_str(), "agent");
        assert_eq!(ParticipantKind::Observer.as_str(), "observer");
    }

    #[test]
    fn test_participant_kind_from_str() {
        assert_eq!("user".parse::<ParticipantKind>().unwrap(), ParticipantKind::User);
        assert_eq!("agent".parse::<ParticipantKind>().unwrap(), ParticipantKind::Agent);
        assert_eq!("observer".parse::<ParticipantKind>().unwrap(), ParticipantKind::Observer);
    }

    #[test]
    fn test_participant_kind_from_str_invalid() {
        let result = "invalid".parse::<ParticipantKind>();
        assert!(result.is_err());
    }

    #[test]
    fn test_participant_status_as_str() {
        assert_eq!(ParticipantStatus::Active.as_str(), "active");
        assert_eq!(ParticipantStatus::Idle.as_str(), "idle");
        assert_eq!(ParticipantStatus::Disconnected.as_str(), "disconnected");
    }

    #[test]
    fn test_participant_status_from_str() {
        assert_eq!("active".parse::<ParticipantStatus>().unwrap(), ParticipantStatus::Active);
        assert_eq!("idle".parse::<ParticipantStatus>().unwrap(), ParticipantStatus::Idle);
        assert_eq!("disconnected".parse::<ParticipantStatus>().unwrap(), ParticipantStatus::Disconnected);
    }

    #[test]
    fn test_participant_status_from_str_invalid() {
        let result = "invalid".parse::<ParticipantStatus>();
        assert!(result.is_err());
    }

    #[test]
    fn test_participant_serialization() {
        let p = Participant::new("Alice", ParticipantKind::User);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("Alice"));
        assert!(json.contains("user"));
        assert!(json.contains("active"));
    }

    #[test]
    fn test_participant_color_deterministic() {
        let id = Uuid::new_v4();
        let p1 = Participant::with_id(id, "A", ParticipantKind::User);
        let p2 = Participant::with_id(id, "B", ParticipantKind::User);
        assert_eq!(p1.color, p2.color);
    }

    #[test]
    fn test_is_stale() {
        let mut p = Participant::new("Test", ParticipantKind::User);

        // Fresh participant should not be stale
        assert!(!p.is_stale(chrono::Duration::seconds(60)));

        // Manually set last_seen_at to the past
        p.last_seen_at = Utc::now() - chrono::Duration::seconds(120);
        assert!(p.is_stale(chrono::Duration::seconds(60)));
    }

    #[test]
    fn test_observer_kind() {
        let p = Participant::new("Monitor", ParticipantKind::Observer);
        assert_eq!(p.kind, ParticipantKind::Observer);
    }

    #[test]
    fn test_cursor_serialization_skip_none() {
        let p = Participant::new("Test", ParticipantKind::User);
        let json = serde_json::to_string(&p).unwrap();

        // cursor_block_id and cursor_offset should be skipped when None
        assert!(!json.contains("cursor_block_id"));
        assert!(!json.contains("cursor_offset"));
    }

    #[test]
    fn test_cursor_serialization_with_values() {
        let mut p = Participant::new("Test", ParticipantKind::User);
        p.set_cursor(Some(Uuid::new_v4()), Some(10));
        let json = serde_json::to_string(&p).unwrap();

        assert!(json.contains("cursor_block_id"));
        assert!(json.contains("cursor_offset"));
    }
}
