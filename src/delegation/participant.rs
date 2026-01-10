//! Registered participant model
//!
//! Extends the basic Participant with capabilities and delegation state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::capability::{Capability, CapabilitySet};
use crate::crdt::{Participant, ParticipantKind};

/// A participant registered with the delegation system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredParticipant {
    /// The underlying participant (presence info)
    pub participant: Participant,
    /// Capabilities granted to this participant
    pub capabilities: CapabilitySet,
    /// Whether this participant can receive delegated work
    pub accepting_work: bool,
    /// Maximum concurrent work items this participant can handle
    pub work_capacity: u32,
    /// When this participant was registered
    pub registered_at: DateTime<Utc>,
}

impl RegisteredParticipant {
    /// Create a new registered participant with default capabilities
    pub fn new(participant: Participant) -> Self {
        let capabilities = match participant.kind {
            ParticipantKind::User => Capability::default_user().into(),
            ParticipantKind::Agent => Capability::default_agent().into(),
            ParticipantKind::Observer => Capability::default_observer().into(),
        };

        let work_capacity = match participant.kind {
            ParticipantKind::User => 5,
            ParticipantKind::Agent => 10,
            ParticipantKind::Observer => 0,
        };

        Self {
            participant,
            capabilities,
            accepting_work: true,
            work_capacity,
            registered_at: Utc::now(),
        }
    }

    /// Create with specific capabilities
    pub fn with_capabilities(participant: Participant, capabilities: CapabilitySet) -> Self {
        let work_capacity = match participant.kind {
            ParticipantKind::User => 5,
            ParticipantKind::Agent => 10,
            ParticipantKind::Observer => 0,
        };

        Self {
            participant,
            capabilities,
            accepting_work: true,
            work_capacity,
            registered_at: Utc::now(),
        }
    }

    /// Get the participant ID
    pub fn id(&self) -> Uuid {
        self.participant.id
    }

    /// Get the participant name
    pub fn name(&self) -> &str {
        &self.participant.name
    }

    /// Get the participant kind
    pub fn kind(&self) -> ParticipantKind {
        self.participant.kind
    }

    /// Check if this participant has a capability
    pub fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.has(cap)
    }

    /// Check if this participant can delegate work
    pub fn can_delegate(&self) -> bool {
        self.has_capability(Capability::Delegate)
    }

    /// Check if this participant can approve work
    pub fn can_approve(&self) -> bool {
        self.has_capability(Capability::Approve)
    }

    /// Check if this participant can receive work
    pub fn can_receive_work(&self) -> bool {
        self.accepting_work && self.work_capacity > 0
    }

    /// Set accepting work status
    pub fn set_accepting_work(&mut self, accepting: bool) {
        self.accepting_work = accepting;
    }

    /// Grant a capability
    pub fn grant_capability(&mut self, cap: Capability) {
        self.capabilities.add(cap);
    }

    /// Revoke a capability
    pub fn revoke_capability(&mut self, cap: Capability) {
        self.capabilities.remove(cap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_participant() -> Participant {
        Participant::new("Alice", ParticipantKind::User)
    }

    fn make_agent_participant() -> Participant {
        Participant::new("Bot", ParticipantKind::Agent)
    }

    fn make_observer_participant() -> Participant {
        Participant::new("Watcher", ParticipantKind::Observer)
    }

    #[test]
    fn test_registered_participant_user_defaults() {
        let p = Participant::new("Alice", ParticipantKind::User);
        let reg = RegisteredParticipant::new(p);

        assert!(reg.has_capability(Capability::Read));
        assert!(reg.has_capability(Capability::Submit));
        assert!(reg.has_capability(Capability::Fork));
        assert!(reg.has_capability(Capability::Delegate));
        assert!(reg.has_capability(Capability::Approve));
        assert!(!reg.has_capability(Capability::Admin));
        assert_eq!(reg.work_capacity, 5);
    }

    #[test]
    fn test_registered_participant_agent_defaults() {
        let p = Participant::new("Bot", ParticipantKind::Agent);
        let reg = RegisteredParticipant::new(p);

        assert!(reg.has_capability(Capability::Read));
        assert!(reg.has_capability(Capability::Submit));
        assert!(reg.has_capability(Capability::Fork));
        assert!(reg.has_capability(Capability::Delegate));
        assert!(!reg.has_capability(Capability::Approve));
        assert!(!reg.has_capability(Capability::Admin));
        assert_eq!(reg.work_capacity, 10);
    }

    #[test]
    fn test_registered_participant_observer_defaults() {
        let p = Participant::new("Watcher", ParticipantKind::Observer);
        let reg = RegisteredParticipant::new(p);

        assert!(reg.has_capability(Capability::Read));
        assert!(!reg.has_capability(Capability::Submit));
        assert!(!reg.has_capability(Capability::Fork));
        assert!(!reg.has_capability(Capability::Delegate));
        assert!(!reg.has_capability(Capability::Approve));
        assert_eq!(reg.work_capacity, 0);
    }

    #[test]
    fn test_registered_participant_with_capabilities() {
        let p = make_user_participant();
        let mut caps = CapabilitySet::new();
        caps.add(Capability::Read);
        caps.add(Capability::Admin);

        let reg = RegisteredParticipant::with_capabilities(p, caps);
        assert!(reg.has_capability(Capability::Admin));
        // Admin grants all
        assert!(reg.has_capability(Capability::Submit));
    }

    #[test]
    fn test_can_delegate() {
        let user = RegisteredParticipant::new(make_user_participant());
        let observer = RegisteredParticipant::new(make_observer_participant());

        assert!(user.can_delegate());
        assert!(!observer.can_delegate());
    }

    #[test]
    fn test_can_approve() {
        let user = RegisteredParticipant::new(make_user_participant());
        let agent = RegisteredParticipant::new(make_agent_participant());

        assert!(user.can_approve());
        assert!(!agent.can_approve());
    }

    #[test]
    fn test_can_receive_work() {
        let mut user = RegisteredParticipant::new(make_user_participant());
        assert!(user.can_receive_work());

        user.set_accepting_work(false);
        assert!(!user.can_receive_work());

        let observer = RegisteredParticipant::new(make_observer_participant());
        assert!(!observer.can_receive_work()); // capacity is 0
    }

    #[test]
    fn test_grant_and_revoke_capability() {
        let mut reg = RegisteredParticipant::new(make_agent_participant());
        assert!(!reg.has_capability(Capability::Approve));

        reg.grant_capability(Capability::Approve);
        assert!(reg.has_capability(Capability::Approve));

        reg.revoke_capability(Capability::Approve);
        assert!(!reg.has_capability(Capability::Approve));
    }

    #[test]
    fn test_participant_id_and_name() {
        let p = Participant::new("TestUser", ParticipantKind::User);
        let id = p.id;
        let reg = RegisteredParticipant::new(p);

        assert_eq!(reg.id(), id);
        assert_eq!(reg.name(), "TestUser");
        assert_eq!(reg.kind(), ParticipantKind::User);
    }

    #[test]
    fn test_serialization() {
        let reg = RegisteredParticipant::new(make_user_participant());
        let json = serde_json::to_string(&reg).unwrap();
        assert!(json.contains("participant"));
        assert!(json.contains("capabilities"));
        assert!(json.contains("accepting_work"));
    }
}
