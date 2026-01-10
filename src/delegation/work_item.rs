//! Work items and approval requests
//!
//! Represents delegated work and approval workflows.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a work item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    /// Work has been delegated but not yet accepted
    Pending,
    /// Work has been accepted and is being worked on
    InProgress,
    /// Work is complete and awaiting approval
    AwaitingApproval,
    /// Work has been approved and is complete
    Approved,
    /// Work was rejected and needs revision
    Rejected,
    /// Work was declined by the assignee
    Declined,
    /// Work was cancelled by the delegator
    Cancelled,
}

impl WorkItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkItemStatus::Pending => "pending",
            WorkItemStatus::InProgress => "in_progress",
            WorkItemStatus::AwaitingApproval => "awaiting_approval",
            WorkItemStatus::Approved => "approved",
            WorkItemStatus::Rejected => "rejected",
            WorkItemStatus::Declined => "declined",
            WorkItemStatus::Cancelled => "cancelled",
        }
    }

    /// Check if this is a terminal status
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkItemStatus::Approved
                | WorkItemStatus::Declined
                | WorkItemStatus::Cancelled
        )
    }

    /// Check if this status allows work to be done
    pub fn is_active(&self) -> bool {
        matches!(self, WorkItemStatus::InProgress | WorkItemStatus::Rejected)
    }
}

impl std::str::FromStr for WorkItemStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(WorkItemStatus::Pending),
            "in_progress" => Ok(WorkItemStatus::InProgress),
            "awaiting_approval" => Ok(WorkItemStatus::AwaitingApproval),
            "approved" => Ok(WorkItemStatus::Approved),
            "rejected" => Ok(WorkItemStatus::Rejected),
            "declined" => Ok(WorkItemStatus::Declined),
            "cancelled" => Ok(WorkItemStatus::Cancelled),
            _ => Err(format!("Invalid work item status: {}", s)),
        }
    }
}

/// Priority level for work items
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

impl Default for WorkPriority {
    fn default() -> Self {
        WorkPriority::Normal
    }
}

impl WorkPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkPriority::Low => "low",
            WorkPriority::Normal => "normal",
            WorkPriority::High => "high",
            WorkPriority::Urgent => "urgent",
        }
    }
}

impl std::str::FromStr for WorkPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(WorkPriority::Low),
            "normal" => Ok(WorkPriority::Normal),
            "high" => Ok(WorkPriority::High),
            "urgent" => Ok(WorkPriority::Urgent),
            _ => Err(format!("Invalid work priority: {}", s)),
        }
    }
}

/// A work item representing delegated work
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    /// Unique identifier
    pub id: Uuid,
    /// The journal this work is associated with
    pub journal_id: Uuid,
    /// Description of the work to be done
    pub description: String,
    /// Optional block ID this work is related to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<Uuid>,
    /// The participant who delegated this work
    pub delegator_id: Uuid,
    /// The participant assigned to do this work
    pub assignee_id: Uuid,
    /// Current status
    pub status: WorkItemStatus,
    /// Priority level
    pub priority: WorkPriority,
    /// Whether approval is required upon completion
    pub requires_approval: bool,
    /// Who should approve (defaults to delegator)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approver_id: Option<Uuid>,
    /// Result/output when work is complete
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl WorkItem {
    /// Create a new work item
    pub fn new(
        journal_id: Uuid,
        description: impl Into<String>,
        delegator_id: Uuid,
        assignee_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            journal_id,
            description: description.into(),
            block_id: None,
            delegator_id,
            assignee_id,
            status: WorkItemStatus::Pending,
            priority: WorkPriority::Normal,
            requires_approval: false,
            approver_id: None,
            result: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create with a specific block
    pub fn for_block(
        journal_id: Uuid,
        block_id: Uuid,
        description: impl Into<String>,
        delegator_id: Uuid,
        assignee_id: Uuid,
    ) -> Self {
        let mut item = Self::new(journal_id, description, delegator_id, assignee_id);
        item.block_id = Some(block_id);
        item
    }

    /// Set priority
    pub fn with_priority(mut self, priority: WorkPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Require approval for this work item
    pub fn require_approval(mut self, approver_id: Option<Uuid>) -> Self {
        self.requires_approval = true;
        self.approver_id = approver_id;
        self
    }

    /// Accept the work item (move to in_progress)
    pub fn accept(&mut self) -> Result<(), String> {
        if self.status != WorkItemStatus::Pending {
            return Err(format!(
                "Cannot accept work item with status: {}",
                self.status.as_str()
            ));
        }
        self.status = WorkItemStatus::InProgress;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Decline the work item
    pub fn decline(&mut self) -> Result<(), String> {
        if self.status != WorkItemStatus::Pending {
            return Err(format!(
                "Cannot decline work item with status: {}",
                self.status.as_str()
            ));
        }
        self.status = WorkItemStatus::Declined;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Submit the work for approval
    pub fn submit_for_approval(&mut self, result: impl Into<String>) -> Result<(), String> {
        if !self.status.is_active() {
            return Err(format!(
                "Cannot submit work item with status: {}",
                self.status.as_str()
            ));
        }
        self.result = Some(result.into());
        if self.requires_approval {
            self.status = WorkItemStatus::AwaitingApproval;
        } else {
            self.status = WorkItemStatus::Approved;
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete work without approval requirement
    pub fn complete(&mut self, result: impl Into<String>) -> Result<(), String> {
        if !self.status.is_active() {
            return Err(format!(
                "Cannot complete work item with status: {}",
                self.status.as_str()
            ));
        }
        self.result = Some(result.into());
        self.status = WorkItemStatus::Approved;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Cancel the work item (by delegator)
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.status.is_terminal() {
            return Err(format!(
                "Cannot cancel work item with terminal status: {}",
                self.status.as_str()
            ));
        }
        self.status = WorkItemStatus::Cancelled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Get the approver ID (defaults to delegator)
    pub fn get_approver_id(&self) -> Uuid {
        self.approver_id.unwrap_or(self.delegator_id)
    }
}

/// Status of an approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Waiting for review
    Pending,
    /// Approved
    Approved,
    /// Rejected with feedback
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
        }
    }
}

impl std::str::FromStr for ApprovalStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ApprovalStatus::Pending),
            "approved" => Ok(ApprovalStatus::Approved),
            "rejected" => Ok(ApprovalStatus::Rejected),
            _ => Err(format!("Invalid approval status: {}", s)),
        }
    }
}

/// An approval request for completed work
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique identifier
    pub id: Uuid,
    /// The work item being approved
    pub work_item_id: Uuid,
    /// The participant requesting approval
    pub requester_id: Uuid,
    /// The participant who should approve
    pub approver_id: Uuid,
    /// Current status
    pub status: ApprovalStatus,
    /// Feedback from the approver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Resolved timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl ApprovalRequest {
    /// Create a new approval request
    pub fn new(work_item: &WorkItem) -> Self {
        Self {
            id: Uuid::new_v4(),
            work_item_id: work_item.id,
            requester_id: work_item.assignee_id,
            approver_id: work_item.get_approver_id(),
            status: ApprovalStatus::Pending,
            feedback: None,
            created_at: Utc::now(),
            resolved_at: None,
        }
    }

    /// Approve the request
    pub fn approve(&mut self, feedback: Option<String>) -> Result<(), String> {
        if self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot approve request with status: {}",
                self.status.as_str()
            ));
        }
        self.status = ApprovalStatus::Approved;
        self.feedback = feedback;
        self.resolved_at = Some(Utc::now());
        Ok(())
    }

    /// Reject the request
    pub fn reject(&mut self, feedback: impl Into<String>) -> Result<(), String> {
        if self.status != ApprovalStatus::Pending {
            return Err(format!(
                "Cannot reject request with status: {}",
                self.status.as_str()
            ));
        }
        self.status = ApprovalStatus::Rejected;
        self.feedback = Some(feedback.into());
        self.resolved_at = Some(Utc::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_work_item() -> WorkItem {
        WorkItem::new(
            Uuid::new_v4(),
            "Test task",
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn test_work_item_status_as_str() {
        assert_eq!(WorkItemStatus::Pending.as_str(), "pending");
        assert_eq!(WorkItemStatus::InProgress.as_str(), "in_progress");
        assert_eq!(WorkItemStatus::AwaitingApproval.as_str(), "awaiting_approval");
        assert_eq!(WorkItemStatus::Approved.as_str(), "approved");
        assert_eq!(WorkItemStatus::Rejected.as_str(), "rejected");
        assert_eq!(WorkItemStatus::Declined.as_str(), "declined");
        assert_eq!(WorkItemStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_work_item_status_from_str() {
        assert_eq!("pending".parse::<WorkItemStatus>().unwrap(), WorkItemStatus::Pending);
        assert_eq!("in_progress".parse::<WorkItemStatus>().unwrap(), WorkItemStatus::InProgress);
    }

    #[test]
    fn test_work_item_status_is_terminal() {
        assert!(!WorkItemStatus::Pending.is_terminal());
        assert!(!WorkItemStatus::InProgress.is_terminal());
        assert!(!WorkItemStatus::AwaitingApproval.is_terminal());
        assert!(WorkItemStatus::Approved.is_terminal());
        assert!(!WorkItemStatus::Rejected.is_terminal());
        assert!(WorkItemStatus::Declined.is_terminal());
        assert!(WorkItemStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_work_item_status_is_active() {
        assert!(!WorkItemStatus::Pending.is_active());
        assert!(WorkItemStatus::InProgress.is_active());
        assert!(!WorkItemStatus::AwaitingApproval.is_active());
        assert!(!WorkItemStatus::Approved.is_active());
        assert!(WorkItemStatus::Rejected.is_active());
    }

    #[test]
    fn test_work_priority_ordering() {
        assert!(WorkPriority::Low < WorkPriority::Normal);
        assert!(WorkPriority::Normal < WorkPriority::High);
        assert!(WorkPriority::High < WorkPriority::Urgent);
    }

    #[test]
    fn test_work_item_new() {
        let journal_id = Uuid::new_v4();
        let delegator_id = Uuid::new_v4();
        let assignee_id = Uuid::new_v4();

        let item = WorkItem::new(journal_id, "Do something", delegator_id, assignee_id);

        assert_eq!(item.journal_id, journal_id);
        assert_eq!(item.description, "Do something");
        assert_eq!(item.delegator_id, delegator_id);
        assert_eq!(item.assignee_id, assignee_id);
        assert_eq!(item.status, WorkItemStatus::Pending);
        assert_eq!(item.priority, WorkPriority::Normal);
        assert!(!item.requires_approval);
    }

    #[test]
    fn test_work_item_for_block() {
        let journal_id = Uuid::new_v4();
        let block_id = Uuid::new_v4();
        let delegator_id = Uuid::new_v4();
        let assignee_id = Uuid::new_v4();

        let item = WorkItem::for_block(journal_id, block_id, "Work on block", delegator_id, assignee_id);

        assert_eq!(item.block_id, Some(block_id));
    }

    #[test]
    fn test_work_item_with_priority() {
        let item = make_work_item().with_priority(WorkPriority::Urgent);
        assert_eq!(item.priority, WorkPriority::Urgent);
    }

    #[test]
    fn test_work_item_require_approval() {
        let approver = Uuid::new_v4();
        let item = make_work_item().require_approval(Some(approver));

        assert!(item.requires_approval);
        assert_eq!(item.approver_id, Some(approver));
    }

    #[test]
    fn test_work_item_accept() {
        let mut item = make_work_item();
        assert!(item.accept().is_ok());
        assert_eq!(item.status, WorkItemStatus::InProgress);
    }

    #[test]
    fn test_work_item_accept_invalid_status() {
        let mut item = make_work_item();
        item.status = WorkItemStatus::InProgress;
        assert!(item.accept().is_err());
    }

    #[test]
    fn test_work_item_decline() {
        let mut item = make_work_item();
        assert!(item.decline().is_ok());
        assert_eq!(item.status, WorkItemStatus::Declined);
    }

    #[test]
    fn test_work_item_complete() {
        let mut item = make_work_item();
        item.accept().unwrap();
        assert!(item.complete("Done!").is_ok());
        assert_eq!(item.status, WorkItemStatus::Approved);
        assert_eq!(item.result, Some("Done!".to_string()));
    }

    #[test]
    fn test_work_item_submit_for_approval() {
        let mut item = make_work_item().require_approval(None);
        item.accept().unwrap();
        assert!(item.submit_for_approval("Please review").is_ok());
        assert_eq!(item.status, WorkItemStatus::AwaitingApproval);
    }

    #[test]
    fn test_work_item_cancel() {
        let mut item = make_work_item();
        assert!(item.cancel().is_ok());
        assert_eq!(item.status, WorkItemStatus::Cancelled);
    }

    #[test]
    fn test_work_item_cancel_terminal() {
        let mut item = make_work_item();
        item.status = WorkItemStatus::Approved;
        assert!(item.cancel().is_err());
    }

    #[test]
    fn test_work_item_get_approver_id() {
        let delegator_id = Uuid::new_v4();
        let assignee_id = Uuid::new_v4();
        let approver_id = Uuid::new_v4();

        let item1 = WorkItem::new(Uuid::new_v4(), "Test", delegator_id, assignee_id);
        assert_eq!(item1.get_approver_id(), delegator_id);

        let item2 = item1.require_approval(Some(approver_id));
        assert_eq!(item2.get_approver_id(), approver_id);
    }

    #[test]
    fn test_approval_request_new() {
        let mut item = make_work_item().require_approval(None);
        item.accept().unwrap();
        item.submit_for_approval("Done").unwrap();

        let request = ApprovalRequest::new(&item);

        assert_eq!(request.work_item_id, item.id);
        assert_eq!(request.requester_id, item.assignee_id);
        assert_eq!(request.approver_id, item.delegator_id);
        assert_eq!(request.status, ApprovalStatus::Pending);
    }

    #[test]
    fn test_approval_request_approve() {
        let item = make_work_item();
        let mut request = ApprovalRequest::new(&item);

        assert!(request.approve(Some("Looks good!".to_string())).is_ok());
        assert_eq!(request.status, ApprovalStatus::Approved);
        assert_eq!(request.feedback, Some("Looks good!".to_string()));
        assert!(request.resolved_at.is_some());
    }

    #[test]
    fn test_approval_request_reject() {
        let item = make_work_item();
        let mut request = ApprovalRequest::new(&item);

        assert!(request.reject("Needs more work").is_ok());
        assert_eq!(request.status, ApprovalStatus::Rejected);
        assert_eq!(request.feedback, Some("Needs more work".to_string()));
    }

    #[test]
    fn test_approval_request_double_approve() {
        let item = make_work_item();
        let mut request = ApprovalRequest::new(&item);
        request.approve(None).unwrap();

        assert!(request.approve(None).is_err());
    }

    #[test]
    fn test_work_item_serialization() {
        let item = make_work_item();
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("description"));
        assert!(json.contains("pending"));
    }

    #[test]
    fn test_approval_status_as_str() {
        assert_eq!(ApprovalStatus::Pending.as_str(), "pending");
        assert_eq!(ApprovalStatus::Approved.as_str(), "approved");
        assert_eq!(ApprovalStatus::Rejected.as_str(), "rejected");
    }
}
