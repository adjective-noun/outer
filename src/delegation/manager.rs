//! Delegation manager for coordinating work delegation and approval
//!
//! The manager handles:
//! - Participant registration and capability management
//! - Work delegation between participants
//! - Work queues per participant
//! - Approval request/response flows
//! - Event broadcasting

use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use super::capability::{Capability, CapabilitySet};
use super::participant::RegisteredParticipant;
use super::work_item::{ApprovalRequest, WorkItem, WorkItemStatus, WorkPriority};
use crate::crdt::{Participant, ParticipantKind};

/// Events emitted by the delegation manager
#[derive(Debug, Clone)]
pub enum DelegationEvent {
    /// A participant was registered
    ParticipantRegistered {
        participant_id: Uuid,
        name: String,
        kind: ParticipantKind,
    },
    /// A participant's capabilities changed
    CapabilitiesChanged {
        participant_id: Uuid,
        capabilities: Vec<Capability>,
    },
    /// Work was delegated to a participant
    WorkDelegated {
        work_item_id: Uuid,
        delegator_id: Uuid,
        assignee_id: Uuid,
        description: String,
    },
    /// Work was accepted by the assignee
    WorkAccepted {
        work_item_id: Uuid,
        assignee_id: Uuid,
    },
    /// Work was declined by the assignee
    WorkDeclined {
        work_item_id: Uuid,
        assignee_id: Uuid,
    },
    /// Work was submitted for approval
    ApprovalRequested {
        approval_id: Uuid,
        work_item_id: Uuid,
        requester_id: Uuid,
        approver_id: Uuid,
    },
    /// Work was approved
    WorkApproved {
        work_item_id: Uuid,
        approver_id: Uuid,
        feedback: Option<String>,
    },
    /// Work was rejected
    WorkRejected {
        work_item_id: Uuid,
        approver_id: Uuid,
        feedback: String,
    },
    /// Work was cancelled
    WorkCancelled {
        work_item_id: Uuid,
        cancelled_by: Uuid,
    },
    /// Work was claimed from the queue
    WorkClaimed {
        work_item_id: Uuid,
        claimed_by: Uuid,
    },
    /// Participant status changed (accepting work or not)
    ParticipantStatusChanged {
        participant_id: Uuid,
        accepting_work: bool,
    },
}

/// Error types for delegation operations
#[derive(Debug, Clone)]
pub enum DelegationError {
    /// Participant not found
    ParticipantNotFound(Uuid),
    /// Work item not found
    WorkItemNotFound(Uuid),
    /// Approval request not found
    ApprovalNotFound(Uuid),
    /// Participant lacks required capability
    InsufficientCapability {
        participant_id: Uuid,
        required: Capability,
    },
    /// Participant not accepting work
    NotAcceptingWork(Uuid),
    /// Invalid state transition
    InvalidStateTransition(String),
    /// Not authorized for this operation
    NotAuthorized(String),
}

impl std::fmt::Display for DelegationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationError::ParticipantNotFound(id) => {
                write!(f, "Participant not found: {}", id)
            }
            DelegationError::WorkItemNotFound(id) => {
                write!(f, "Work item not found: {}", id)
            }
            DelegationError::ApprovalNotFound(id) => {
                write!(f, "Approval request not found: {}", id)
            }
            DelegationError::InsufficientCapability {
                participant_id,
                required,
            } => {
                write!(
                    f,
                    "Participant {} lacks capability: {}",
                    participant_id,
                    required.as_str()
                )
            }
            DelegationError::NotAcceptingWork(id) => {
                write!(f, "Participant {} is not accepting work", id)
            }
            DelegationError::InvalidStateTransition(msg) => {
                write!(f, "Invalid state transition: {}", msg)
            }
            DelegationError::NotAuthorized(msg) => {
                write!(f, "Not authorized: {}", msg)
            }
        }
    }
}

impl std::error::Error for DelegationError {}

/// Result type for delegation operations
pub type DelegationResult<T> = Result<T, DelegationError>;

/// Manager for delegation and approval workflows
pub struct DelegationManager {
    /// Registered participants by ID
    participants: RwLock<HashMap<Uuid, RegisteredParticipant>>,
    /// Work items by ID
    work_items: RwLock<HashMap<Uuid, WorkItem>>,
    /// Approval requests by ID
    approvals: RwLock<HashMap<Uuid, ApprovalRequest>>,
    /// Work queue per participant (assignee_id -> work_item_ids)
    work_queues: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Pending approvals per participant (approver_id -> approval_ids)
    approval_queues: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<DelegationEvent>,
}

impl DelegationManager {
    /// Create a new delegation manager
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            participants: RwLock::new(HashMap::new()),
            work_items: RwLock::new(HashMap::new()),
            approvals: RwLock::new(HashMap::new()),
            work_queues: RwLock::new(HashMap::new()),
            approval_queues: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Subscribe to delegation events
    pub fn subscribe(&self) -> broadcast::Receiver<DelegationEvent> {
        self.event_tx.subscribe()
    }

    /// Register a participant with the delegation system
    pub async fn register_participant(&self, participant: Participant) -> RegisteredParticipant {
        let registered = RegisteredParticipant::new(participant);
        let id = registered.id();
        let name = registered.name().to_string();
        let kind = registered.kind();

        {
            let mut participants = self.participants.write().await;
            participants.insert(id, registered.clone());
        }

        {
            let mut queues = self.work_queues.write().await;
            queues.entry(id).or_default();
        }

        {
            let mut queues = self.approval_queues.write().await;
            queues.entry(id).or_default();
        }

        let _ = self.event_tx.send(DelegationEvent::ParticipantRegistered {
            participant_id: id,
            name,
            kind,
        });

        registered
    }

    /// Register a participant with specific capabilities
    pub async fn register_participant_with_capabilities(
        &self,
        participant: Participant,
        capabilities: CapabilitySet,
    ) -> RegisteredParticipant {
        let registered = RegisteredParticipant::with_capabilities(participant, capabilities);
        let id = registered.id();
        let name = registered.name().to_string();
        let kind = registered.kind();

        {
            let mut participants = self.participants.write().await;
            participants.insert(id, registered.clone());
        }

        {
            let mut queues = self.work_queues.write().await;
            queues.entry(id).or_default();
        }

        {
            let mut queues = self.approval_queues.write().await;
            queues.entry(id).or_default();
        }

        let _ = self.event_tx.send(DelegationEvent::ParticipantRegistered {
            participant_id: id,
            name,
            kind,
        });

        registered
    }

    /// Get a participant by ID
    pub async fn get_participant(&self, id: Uuid) -> Option<RegisteredParticipant> {
        let participants = self.participants.read().await;
        participants.get(&id).cloned()
    }

    /// Unregister a participant
    pub async fn unregister_participant(&self, id: Uuid) -> Option<RegisteredParticipant> {
        let mut participants = self.participants.write().await;
        participants.remove(&id)
    }

    /// Update participant capabilities
    pub async fn update_capabilities(
        &self,
        participant_id: Uuid,
        capabilities: CapabilitySet,
    ) -> DelegationResult<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or(DelegationError::ParticipantNotFound(participant_id))?;

        participant.capabilities = capabilities.clone();

        let _ = self.event_tx.send(DelegationEvent::CapabilitiesChanged {
            participant_id,
            capabilities: capabilities.to_vec(),
        });

        Ok(())
    }

    /// Set whether a participant is accepting work
    pub async fn set_accepting_work(
        &self,
        participant_id: Uuid,
        accepting: bool,
    ) -> DelegationResult<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or(DelegationError::ParticipantNotFound(participant_id))?;

        participant.set_accepting_work(accepting);

        let _ = self
            .event_tx
            .send(DelegationEvent::ParticipantStatusChanged {
                participant_id,
                accepting_work: accepting,
            });

        Ok(())
    }

    /// Delegate work to a participant
    #[allow(clippy::too_many_arguments)]
    pub async fn delegate(
        &self,
        journal_id: Uuid,
        description: impl Into<String>,
        delegator_id: Uuid,
        assignee_id: Uuid,
        priority: Option<WorkPriority>,
        requires_approval: bool,
        approver_id: Option<Uuid>,
    ) -> DelegationResult<WorkItem> {
        let description = description.into();

        // Check delegator has delegate capability
        {
            let participants = self.participants.read().await;
            let delegator = participants
                .get(&delegator_id)
                .ok_or(DelegationError::ParticipantNotFound(delegator_id))?;

            if !delegator.can_delegate() {
                return Err(DelegationError::InsufficientCapability {
                    participant_id: delegator_id,
                    required: Capability::Delegate,
                });
            }

            let assignee = participants
                .get(&assignee_id)
                .ok_or(DelegationError::ParticipantNotFound(assignee_id))?;

            if !assignee.can_receive_work() {
                return Err(DelegationError::NotAcceptingWork(assignee_id));
            }
        }

        // Create work item
        let mut work_item = WorkItem::new(journal_id, &description, delegator_id, assignee_id);
        if let Some(p) = priority {
            work_item = work_item.with_priority(p);
        }
        if requires_approval {
            work_item = work_item.require_approval(approver_id);
        }

        let work_item_id = work_item.id;

        // Store work item
        {
            let mut items = self.work_items.write().await;
            items.insert(work_item_id, work_item.clone());
        }

        // Add to assignee's queue
        {
            let mut queues = self.work_queues.write().await;
            queues.entry(assignee_id).or_default().push(work_item_id);
        }

        let _ = self.event_tx.send(DelegationEvent::WorkDelegated {
            work_item_id,
            delegator_id,
            assignee_id,
            description,
        });

        Ok(work_item)
    }

    /// Accept a delegated work item
    pub async fn accept_work(
        &self,
        work_item_id: Uuid,
        acceptor_id: Uuid,
    ) -> DelegationResult<WorkItem> {
        let mut items = self.work_items.write().await;
        let item = items
            .get_mut(&work_item_id)
            .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

        // Verify acceptor is the assignee
        if item.assignee_id != acceptor_id {
            return Err(DelegationError::NotAuthorized(
                "Only the assignee can accept work".to_string(),
            ));
        }

        item.accept()
            .map_err(DelegationError::InvalidStateTransition)?;

        let _ = self.event_tx.send(DelegationEvent::WorkAccepted {
            work_item_id,
            assignee_id: acceptor_id,
        });

        Ok(item.clone())
    }

    /// Decline a delegated work item
    pub async fn decline_work(
        &self,
        work_item_id: Uuid,
        decliner_id: Uuid,
    ) -> DelegationResult<WorkItem> {
        let item = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            if item.assignee_id != decliner_id {
                return Err(DelegationError::NotAuthorized(
                    "Only the assignee can decline work".to_string(),
                ));
            }

            item.decline()
                .map_err(DelegationError::InvalidStateTransition)?;

            item.clone()
        };

        // Remove from queue
        {
            let mut queues = self.work_queues.write().await;
            if let Some(queue) = queues.get_mut(&decliner_id) {
                queue.retain(|&id| id != work_item_id);
            }
        }

        let _ = self.event_tx.send(DelegationEvent::WorkDeclined {
            work_item_id,
            assignee_id: decliner_id,
        });

        Ok(item)
    }

    /// Submit work for approval (or complete if no approval required)
    pub async fn submit_work(
        &self,
        work_item_id: Uuid,
        submitter_id: Uuid,
        result: impl Into<String>,
    ) -> DelegationResult<WorkItem> {
        let result = result.into();
        let (item, needs_approval) = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            if item.assignee_id != submitter_id {
                return Err(DelegationError::NotAuthorized(
                    "Only the assignee can submit work".to_string(),
                ));
            }

            let needs_approval = item.requires_approval;
            item.submit_for_approval(&result)
                .map_err(DelegationError::InvalidStateTransition)?;

            (item.clone(), needs_approval)
        };

        // Remove from work queue
        {
            let mut queues = self.work_queues.write().await;
            if let Some(queue) = queues.get_mut(&submitter_id) {
                queue.retain(|&id| id != work_item_id);
            }
        }

        // Create approval request if needed
        if needs_approval {
            let approval = ApprovalRequest::new(&item);
            let approval_id = approval.id;
            let approver_id = approval.approver_id;

            {
                let mut approvals = self.approvals.write().await;
                approvals.insert(approval_id, approval);
            }

            {
                let mut queues = self.approval_queues.write().await;
                queues.entry(approver_id).or_default().push(approval_id);
            }

            let _ = self.event_tx.send(DelegationEvent::ApprovalRequested {
                approval_id,
                work_item_id,
                requester_id: submitter_id,
                approver_id,
            });
        } else {
            let _ = self.event_tx.send(DelegationEvent::WorkApproved {
                work_item_id,
                approver_id: item.delegator_id,
                feedback: None,
            });
        }

        Ok(item)
    }

    /// Approve a work item
    pub async fn approve(
        &self,
        approval_id: Uuid,
        approver_id: Uuid,
        feedback: Option<String>,
    ) -> DelegationResult<(ApprovalRequest, WorkItem)> {
        // Check approver has approve capability
        {
            let participants = self.participants.read().await;
            let approver = participants
                .get(&approver_id)
                .ok_or(DelegationError::ParticipantNotFound(approver_id))?;

            if !approver.can_approve() {
                return Err(DelegationError::InsufficientCapability {
                    participant_id: approver_id,
                    required: Capability::Approve,
                });
            }
        }

        let work_item_id = {
            let mut approvals = self.approvals.write().await;
            let approval = approvals
                .get_mut(&approval_id)
                .ok_or(DelegationError::ApprovalNotFound(approval_id))?;

            if approval.approver_id != approver_id {
                return Err(DelegationError::NotAuthorized(
                    "Not the designated approver".to_string(),
                ));
            }

            approval
                .approve(feedback.clone())
                .map_err(DelegationError::InvalidStateTransition)?;

            approval.work_item_id
        };

        // Update work item status
        let item = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            item.status = WorkItemStatus::Approved;
            item.updated_at = chrono::Utc::now();
            item.clone()
        };

        // Remove from approval queue
        {
            let mut queues = self.approval_queues.write().await;
            if let Some(queue) = queues.get_mut(&approver_id) {
                queue.retain(|&id| id != approval_id);
            }
        }

        let approval = {
            let approvals = self.approvals.read().await;
            approvals.get(&approval_id).cloned().unwrap()
        };

        let _ = self.event_tx.send(DelegationEvent::WorkApproved {
            work_item_id,
            approver_id,
            feedback,
        });

        Ok((approval, item))
    }

    /// Reject a work item
    pub async fn reject(
        &self,
        approval_id: Uuid,
        rejecter_id: Uuid,
        feedback: impl Into<String>,
    ) -> DelegationResult<(ApprovalRequest, WorkItem)> {
        let feedback = feedback.into();

        // Check rejecter has approve capability
        {
            let participants = self.participants.read().await;
            let rejecter = participants
                .get(&rejecter_id)
                .ok_or(DelegationError::ParticipantNotFound(rejecter_id))?;

            if !rejecter.can_approve() {
                return Err(DelegationError::InsufficientCapability {
                    participant_id: rejecter_id,
                    required: Capability::Approve,
                });
            }
        }

        let (work_item_id, assignee_id) = {
            let mut approvals = self.approvals.write().await;
            let approval = approvals
                .get_mut(&approval_id)
                .ok_or(DelegationError::ApprovalNotFound(approval_id))?;

            if approval.approver_id != rejecter_id {
                return Err(DelegationError::NotAuthorized(
                    "Not the designated approver".to_string(),
                ));
            }

            approval
                .reject(&feedback)
                .map_err(DelegationError::InvalidStateTransition)?;

            (approval.work_item_id, approval.requester_id)
        };

        // Update work item status back to rejected (can be reworked)
        let item = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            item.status = WorkItemStatus::Rejected;
            item.updated_at = chrono::Utc::now();
            item.clone()
        };

        // Re-add to work queue for rework
        {
            let mut queues = self.work_queues.write().await;
            queues.entry(assignee_id).or_default().push(work_item_id);
        }

        // Remove from approval queue
        {
            let mut queues = self.approval_queues.write().await;
            if let Some(queue) = queues.get_mut(&rejecter_id) {
                queue.retain(|&id| id != approval_id);
            }
        }

        let approval = {
            let approvals = self.approvals.read().await;
            approvals.get(&approval_id).cloned().unwrap()
        };

        let _ = self.event_tx.send(DelegationEvent::WorkRejected {
            work_item_id,
            approver_id: rejecter_id,
            feedback,
        });

        Ok((approval, item))
    }

    /// Cancel a work item (by delegator)
    pub async fn cancel_work(
        &self,
        work_item_id: Uuid,
        canceller_id: Uuid,
    ) -> DelegationResult<WorkItem> {
        let item = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            // Only delegator can cancel
            if item.delegator_id != canceller_id {
                return Err(DelegationError::NotAuthorized(
                    "Only the delegator can cancel work".to_string(),
                ));
            }

            item.cancel()
                .map_err(DelegationError::InvalidStateTransition)?;

            item.clone()
        };

        // Remove from assignee's queue
        {
            let mut queues = self.work_queues.write().await;
            if let Some(queue) = queues.get_mut(&item.assignee_id) {
                queue.retain(|&id| id != work_item_id);
            }
        }

        let _ = self.event_tx.send(DelegationEvent::WorkCancelled {
            work_item_id,
            cancelled_by: canceller_id,
        });

        Ok(item)
    }

    /// Claim an unassigned work item from the general queue
    pub async fn claim_work(
        &self,
        work_item_id: Uuid,
        claimer_id: Uuid,
    ) -> DelegationResult<WorkItem> {
        // Check claimer can receive work
        {
            let participants = self.participants.read().await;
            let claimer = participants
                .get(&claimer_id)
                .ok_or(DelegationError::ParticipantNotFound(claimer_id))?;

            if !claimer.can_receive_work() {
                return Err(DelegationError::NotAcceptingWork(claimer_id));
            }
        }

        let item = {
            let mut items = self.work_items.write().await;
            let item = items
                .get_mut(&work_item_id)
                .ok_or(DelegationError::WorkItemNotFound(work_item_id))?;

            // Can only claim pending work
            if item.status != WorkItemStatus::Pending {
                return Err(DelegationError::InvalidStateTransition(
                    "Can only claim pending work".to_string(),
                ));
            }

            let old_assignee = item.assignee_id;

            // Remove from old assignee's queue
            {
                let mut queues = self.work_queues.write().await;
                if let Some(queue) = queues.get_mut(&old_assignee) {
                    queue.retain(|&id| id != work_item_id);
                }
            }

            // Update assignee
            item.assignee_id = claimer_id;
            item.updated_at = chrono::Utc::now();

            item.clone()
        };

        // Add to claimer's queue
        {
            let mut queues = self.work_queues.write().await;
            queues.entry(claimer_id).or_default().push(work_item_id);
        }

        let _ = self.event_tx.send(DelegationEvent::WorkClaimed {
            work_item_id,
            claimed_by: claimer_id,
        });

        Ok(item)
    }

    /// Get a participant's work queue
    pub async fn get_work_queue(&self, participant_id: Uuid) -> Vec<WorkItem> {
        let queues = self.work_queues.read().await;
        let items = self.work_items.read().await;

        queues
            .get(&participant_id)
            .map(|queue| {
                queue
                    .iter()
                    .filter_map(|id| items.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a participant's pending approval requests
    pub async fn get_approval_queue(&self, participant_id: Uuid) -> Vec<ApprovalRequest> {
        let queues = self.approval_queues.read().await;
        let approvals = self.approvals.read().await;

        queues
            .get(&participant_id)
            .map(|queue| {
                queue
                    .iter()
                    .filter_map(|id| approvals.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a work item by ID
    pub async fn get_work_item(&self, id: Uuid) -> Option<WorkItem> {
        let items = self.work_items.read().await;
        items.get(&id).cloned()
    }

    /// Get an approval request by ID
    pub async fn get_approval(&self, id: Uuid) -> Option<ApprovalRequest> {
        let approvals = self.approvals.read().await;
        approvals.get(&id).cloned()
    }

    /// Get all registered participants
    pub async fn list_participants(&self) -> Vec<RegisteredParticipant> {
        let participants = self.participants.read().await;
        participants.values().cloned().collect()
    }

    /// Get participants accepting work
    pub async fn list_available_participants(&self) -> Vec<RegisteredParticipant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.can_receive_work())
            .cloned()
            .collect()
    }
}

impl Default for DelegationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::work_item::ApprovalStatus;
    use super::*;

    fn make_user() -> Participant {
        Participant::new("Alice", ParticipantKind::User)
    }

    fn make_agent() -> Participant {
        Participant::new("Bot", ParticipantKind::Agent)
    }

    #[tokio::test]
    async fn test_register_participant() {
        let manager = DelegationManager::new();
        let mut rx = manager.subscribe();

        let user = make_user();
        let registered = manager.register_participant(user.clone()).await;

        assert_eq!(registered.id(), user.id);
        assert!(registered.can_delegate());

        // Check event was sent
        let event = rx.try_recv().unwrap();
        match event {
            DelegationEvent::ParticipantRegistered { participant_id, .. } => {
                assert_eq!(participant_id, user.id);
            }
            _ => panic!("Expected ParticipantRegistered event"),
        }
    }

    #[tokio::test]
    async fn test_delegate_work() {
        let manager = DelegationManager::new();
        let mut rx = manager.subscribe();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;
        let _ = rx.try_recv(); // consume register events
        let _ = rx.try_recv();

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Do something",
                user.id(),
                agent.id(),
                Some(WorkPriority::High),
                false,
                None,
            )
            .await
            .unwrap();

        assert_eq!(work.status, WorkItemStatus::Pending);
        assert_eq!(work.priority, WorkPriority::High);

        let event = rx.try_recv().unwrap();
        match event {
            DelegationEvent::WorkDelegated {
                delegator_id,
                assignee_id,
                ..
            } => {
                assert_eq!(delegator_id, user.id());
                assert_eq!(assignee_id, agent.id());
            }
            _ => panic!("Expected WorkDelegated event"),
        }
    }

    #[tokio::test]
    async fn test_accept_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        let accepted = manager.accept_work(work.id, agent.id()).await.unwrap();
        assert_eq!(accepted.status, WorkItemStatus::InProgress);
    }

    #[tokio::test]
    async fn test_decline_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        let declined = manager.decline_work(work.id, agent.id()).await.unwrap();
        assert_eq!(declined.status, WorkItemStatus::Declined);

        // Should be removed from queue
        let queue = manager.get_work_queue(agent.id()).await;
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_submit_work_no_approval() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        manager.accept_work(work.id, agent.id()).await.unwrap();
        let submitted = manager
            .submit_work(work.id, agent.id(), "Done!")
            .await
            .unwrap();

        assert_eq!(submitted.status, WorkItemStatus::Approved);
    }

    #[tokio::test]
    async fn test_submit_work_with_approval() {
        let manager = DelegationManager::new();
        let mut rx = manager.subscribe();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;
        let _ = rx.try_recv();
        let _ = rx.try_recv();

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                true,
                None,
            )
            .await
            .unwrap();
        let _ = rx.try_recv();

        manager.accept_work(work.id, agent.id()).await.unwrap();
        let _ = rx.try_recv();

        let submitted = manager
            .submit_work(work.id, agent.id(), "Done!")
            .await
            .unwrap();

        assert_eq!(submitted.status, WorkItemStatus::AwaitingApproval);

        // Check approval request was created
        let event = rx.try_recv().unwrap();
        match event {
            DelegationEvent::ApprovalRequested {
                work_item_id,
                approver_id,
                ..
            } => {
                assert_eq!(work_item_id, work.id);
                assert_eq!(approver_id, user.id());
            }
            _ => panic!("Expected ApprovalRequested event"),
        }

        // User should have pending approval
        let approvals = manager.get_approval_queue(user.id()).await;
        assert_eq!(approvals.len(), 1);
    }

    #[tokio::test]
    async fn test_approve_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                true,
                None,
            )
            .await
            .unwrap();

        manager.accept_work(work.id, agent.id()).await.unwrap();
        manager
            .submit_work(work.id, agent.id(), "Done!")
            .await
            .unwrap();

        let approvals = manager.get_approval_queue(user.id()).await;
        let approval_id = approvals[0].id;

        let (approval, item) = manager
            .approve(approval_id, user.id(), Some("Good job!".to_string()))
            .await
            .unwrap();

        assert_eq!(approval.status, ApprovalStatus::Approved);
        assert_eq!(item.status, WorkItemStatus::Approved);
    }

    #[tokio::test]
    async fn test_reject_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                true,
                None,
            )
            .await
            .unwrap();

        manager.accept_work(work.id, agent.id()).await.unwrap();
        manager
            .submit_work(work.id, agent.id(), "Done!")
            .await
            .unwrap();

        let approvals = manager.get_approval_queue(user.id()).await;
        let approval_id = approvals[0].id;

        let (approval, item) = manager
            .reject(approval_id, user.id(), "Needs more work")
            .await
            .unwrap();

        assert_eq!(approval.status, ApprovalStatus::Rejected);
        assert_eq!(item.status, WorkItemStatus::Rejected);

        // Work should be back in agent's queue for rework
        let queue = manager.get_work_queue(agent.id()).await;
        assert_eq!(queue.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        let cancelled = manager.cancel_work(work.id, user.id()).await.unwrap();
        assert_eq!(cancelled.status, WorkItemStatus::Cancelled);

        // Should be removed from queue
        let queue = manager.get_work_queue(agent.id()).await;
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_cancel_work_unauthorized() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        // Agent cannot cancel
        let result = manager.cancel_work(work.id, agent.id()).await;
        assert!(matches!(result, Err(DelegationError::NotAuthorized(_))));
    }

    #[tokio::test]
    async fn test_symmetric_delegation_human_to_human() {
        let manager = DelegationManager::new();

        let alice = manager
            .register_participant(Participant::new("Alice", ParticipantKind::User))
            .await;
        let bob = manager
            .register_participant(Participant::new("Bob", ParticipantKind::User))
            .await;

        // Human can delegate to human
        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Review PR",
                alice.id(),
                bob.id(),
                None,
                true,
                None,
            )
            .await
            .unwrap();

        assert_eq!(work.delegator_id, alice.id());
        assert_eq!(work.assignee_id, bob.id());
    }

    #[tokio::test]
    async fn test_symmetric_delegation_agent_to_human() {
        let manager = DelegationManager::new();

        let agent = manager.register_participant(make_agent()).await;
        let user = manager.register_participant(make_user()).await;

        // Agent can delegate back to human
        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Need clarification",
                agent.id(),
                user.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        assert_eq!(work.delegator_id, agent.id());
        assert_eq!(work.assignee_id, user.id());
    }

    #[tokio::test]
    async fn test_symmetric_delegation_agent_to_agent() {
        let manager = DelegationManager::new();

        let agent1 = manager
            .register_participant(Participant::new("Bot1", ParticipantKind::Agent))
            .await;
        let agent2 = manager
            .register_participant(Participant::new("Bot2", ParticipantKind::Agent))
            .await;

        // Agent can delegate to another agent
        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Subtask",
                agent1.id(),
                agent2.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        assert_eq!(work.delegator_id, agent1.id());
        assert_eq!(work.assignee_id, agent2.id());
    }

    #[tokio::test]
    async fn test_observer_cannot_delegate() {
        let manager = DelegationManager::new();

        let observer = manager
            .register_participant(Participant::new("Watcher", ParticipantKind::Observer))
            .await;
        let user = manager.register_participant(make_user()).await;

        let result = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                observer.id(),
                user.id(),
                None,
                false,
                None,
            )
            .await;

        assert!(matches!(
            result,
            Err(DelegationError::InsufficientCapability { .. })
        ));
    }

    #[tokio::test]
    async fn test_claim_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent1 = manager
            .register_participant(Participant::new("Bot1", ParticipantKind::Agent))
            .await;
        let agent2 = manager
            .register_participant(Participant::new("Bot2", ParticipantKind::Agent))
            .await;

        let work = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent1.id(),
                None,
                false,
                None,
            )
            .await
            .unwrap();

        // Agent2 claims the work
        let claimed = manager.claim_work(work.id, agent2.id()).await.unwrap();
        assert_eq!(claimed.assignee_id, agent2.id());

        // Should be in agent2's queue, not agent1's
        let queue1 = manager.get_work_queue(agent1.id()).await;
        let queue2 = manager.get_work_queue(agent2.id()).await;
        assert!(queue1.is_empty());
        assert_eq!(queue2.len(), 1);
    }

    #[tokio::test]
    async fn test_set_accepting_work() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        // Disable accepting work
        manager.set_accepting_work(agent.id(), false).await.unwrap();

        // Now delegation should fail
        let result = manager
            .delegate(
                Uuid::new_v4(),
                "Task",
                user.id(),
                agent.id(),
                None,
                false,
                None,
            )
            .await;

        assert!(matches!(result, Err(DelegationError::NotAcceptingWork(_))));
    }

    #[tokio::test]
    async fn test_list_available_participants() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;
        let observer = manager
            .register_participant(Participant::new("Watcher", ParticipantKind::Observer))
            .await;

        // Disable agent
        manager.set_accepting_work(agent.id(), false).await.unwrap();

        let available = manager.list_available_participants().await;

        // Only user should be available (observer has 0 capacity)
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id(), user.id());
    }

    #[tokio::test]
    async fn test_update_capabilities() {
        let manager = DelegationManager::new();
        let mut rx = manager.subscribe();

        let agent = manager.register_participant(make_agent()).await;
        let _ = rx.try_recv();

        assert!(!agent.has_capability(Capability::Approve));

        // Grant approve capability
        let mut caps = CapabilitySet::new();
        caps.add(Capability::Read);
        caps.add(Capability::Submit);
        caps.add(Capability::Approve);

        manager.update_capabilities(agent.id(), caps).await.unwrap();

        let updated = manager.get_participant(agent.id()).await.unwrap();
        assert!(updated.has_capability(Capability::Approve));

        let event = rx.try_recv().unwrap();
        match event {
            DelegationEvent::CapabilitiesChanged {
                participant_id,
                capabilities,
            } => {
                assert_eq!(participant_id, agent.id());
                assert!(capabilities.contains(&Capability::Approve));
            }
            _ => panic!("Expected CapabilitiesChanged event"),
        }
    }

    #[tokio::test]
    async fn test_work_queue_priority_ordering() {
        let manager = DelegationManager::new();

        let user = manager.register_participant(make_user()).await;
        let agent = manager.register_participant(make_agent()).await;

        // Create work items with different priorities
        let low = manager
            .delegate(
                Uuid::new_v4(),
                "Low",
                user.id(),
                agent.id(),
                Some(WorkPriority::Low),
                false,
                None,
            )
            .await
            .unwrap();
        let high = manager
            .delegate(
                Uuid::new_v4(),
                "High",
                user.id(),
                agent.id(),
                Some(WorkPriority::High),
                false,
                None,
            )
            .await
            .unwrap();
        let normal = manager
            .delegate(
                Uuid::new_v4(),
                "Normal",
                user.id(),
                agent.id(),
                Some(WorkPriority::Normal),
                false,
                None,
            )
            .await
            .unwrap();

        let queue = manager.get_work_queue(agent.id()).await;
        assert_eq!(queue.len(), 3);

        // Items should be in insertion order in queue (priority handled by consumer)
        assert_eq!(queue[0].id, low.id);
        assert_eq!(queue[1].id, high.id);
        assert_eq!(queue[2].id, normal.id);
    }
}
