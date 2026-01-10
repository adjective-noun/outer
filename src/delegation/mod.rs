//! Delegation and approval system for symmetric human-agent collaboration
//!
//! This module implements a capability-based delegation system where any participant
//! (human or agent) can delegate work to any other participant with appropriate permissions.

pub mod capability;
pub mod manager;
pub mod participant;
pub mod work_item;

pub use capability::Capability;
pub use manager::{DelegationManager, DelegationEvent};
pub use participant::RegisteredParticipant;
pub use work_item::{WorkItem, WorkItemStatus, ApprovalRequest, ApprovalStatus};
