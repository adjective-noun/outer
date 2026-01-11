//! Capability model for participants
//!
//! Defines the set of capabilities that can be granted to participants.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Capabilities that can be assigned to participants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Can read journal content and view participants
    Read,
    /// Can submit new messages/prompts
    Submit,
    /// Can fork blocks to create new conversation branches
    Fork,
    /// Can delegate work to other participants
    Delegate,
    /// Can approve or reject work submitted by others
    Approve,
    /// Full administrative access (includes all other capabilities)
    Admin,
}

impl Capability {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Submit => "submit",
            Capability::Fork => "fork",
            Capability::Delegate => "delegate",
            Capability::Approve => "approve",
            Capability::Admin => "admin",
        }
    }

    /// Get all capabilities
    pub fn all() -> HashSet<Capability> {
        let mut caps = HashSet::new();
        caps.insert(Capability::Read);
        caps.insert(Capability::Submit);
        caps.insert(Capability::Fork);
        caps.insert(Capability::Delegate);
        caps.insert(Capability::Approve);
        caps.insert(Capability::Admin);
        caps
    }

    /// Get default capabilities for a human user
    pub fn default_user() -> HashSet<Capability> {
        let mut caps = HashSet::new();
        caps.insert(Capability::Read);
        caps.insert(Capability::Submit);
        caps.insert(Capability::Fork);
        caps.insert(Capability::Delegate);
        caps.insert(Capability::Approve);
        caps
    }

    /// Get default capabilities for an agent
    pub fn default_agent() -> HashSet<Capability> {
        let mut caps = HashSet::new();
        caps.insert(Capability::Read);
        caps.insert(Capability::Submit);
        caps.insert(Capability::Fork);
        // Agents can delegate to other agents or back to humans
        caps.insert(Capability::Delegate);
        caps
    }

    /// Get default capabilities for an observer
    pub fn default_observer() -> HashSet<Capability> {
        let mut caps = HashSet::new();
        caps.insert(Capability::Read);
        caps
    }
}

impl std::str::FromStr for Capability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(Capability::Read),
            "submit" => Ok(Capability::Submit),
            "fork" => Ok(Capability::Fork),
            "delegate" => Ok(Capability::Delegate),
            "approve" => Ok(Capability::Approve),
            "admin" => Ok(Capability::Admin),
            _ => Err(format!("Invalid capability: {}", s)),
        }
    }
}

/// A set of capabilities with helper methods
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    /// Create a new empty capability set
    pub fn new() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    /// Create from a hash set
    pub fn from_set(capabilities: HashSet<Capability>) -> Self {
        Self { capabilities }
    }

    /// Check if this set contains a capability (respects Admin override)
    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&Capability::Admin) || self.capabilities.contains(&cap)
    }

    /// Add a capability
    pub fn add(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    /// Remove a capability
    pub fn remove(&mut self, cap: Capability) {
        self.capabilities.remove(&cap);
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Get the underlying set
    pub fn inner(&self) -> &HashSet<Capability> {
        &self.capabilities
    }

    /// Convert to a vec for serialization
    pub fn to_vec(&self) -> Vec<Capability> {
        self.capabilities.iter().copied().collect()
    }
}

impl From<HashSet<Capability>> for CapabilitySet {
    fn from(capabilities: HashSet<Capability>) -> Self {
        Self { capabilities }
    }
}

impl From<Vec<Capability>> for CapabilitySet {
    fn from(capabilities: Vec<Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_as_str() {
        assert_eq!(Capability::Read.as_str(), "read");
        assert_eq!(Capability::Submit.as_str(), "submit");
        assert_eq!(Capability::Fork.as_str(), "fork");
        assert_eq!(Capability::Delegate.as_str(), "delegate");
        assert_eq!(Capability::Approve.as_str(), "approve");
        assert_eq!(Capability::Admin.as_str(), "admin");
    }

    #[test]
    fn test_capability_from_str() {
        assert_eq!("read".parse::<Capability>().unwrap(), Capability::Read);
        assert_eq!("submit".parse::<Capability>().unwrap(), Capability::Submit);
        assert_eq!("fork".parse::<Capability>().unwrap(), Capability::Fork);
        assert_eq!(
            "delegate".parse::<Capability>().unwrap(),
            Capability::Delegate
        );
        assert_eq!(
            "approve".parse::<Capability>().unwrap(),
            Capability::Approve
        );
        assert_eq!("admin".parse::<Capability>().unwrap(), Capability::Admin);
    }

    #[test]
    fn test_capability_from_str_invalid() {
        let result = "invalid".parse::<Capability>();
        assert!(result.is_err());
    }

    #[test]
    fn test_default_user_capabilities() {
        let caps = Capability::default_user();
        assert!(caps.contains(&Capability::Read));
        assert!(caps.contains(&Capability::Submit));
        assert!(caps.contains(&Capability::Fork));
        assert!(caps.contains(&Capability::Delegate));
        assert!(caps.contains(&Capability::Approve));
        assert!(!caps.contains(&Capability::Admin));
    }

    #[test]
    fn test_default_agent_capabilities() {
        let caps = Capability::default_agent();
        assert!(caps.contains(&Capability::Read));
        assert!(caps.contains(&Capability::Submit));
        assert!(caps.contains(&Capability::Fork));
        assert!(caps.contains(&Capability::Delegate));
        assert!(!caps.contains(&Capability::Approve));
        assert!(!caps.contains(&Capability::Admin));
    }

    #[test]
    fn test_default_observer_capabilities() {
        let caps = Capability::default_observer();
        assert!(caps.contains(&Capability::Read));
        assert!(!caps.contains(&Capability::Submit));
        assert!(!caps.contains(&Capability::Fork));
        assert!(!caps.contains(&Capability::Delegate));
        assert!(!caps.contains(&Capability::Approve));
        assert!(!caps.contains(&Capability::Admin));
    }

    #[test]
    fn test_capability_set_admin_override() {
        let mut caps = CapabilitySet::new();
        caps.add(Capability::Admin);

        // Admin should grant all capabilities
        assert!(caps.has(Capability::Read));
        assert!(caps.has(Capability::Submit));
        assert!(caps.has(Capability::Fork));
        assert!(caps.has(Capability::Delegate));
        assert!(caps.has(Capability::Approve));
        assert!(caps.has(Capability::Admin));
    }

    #[test]
    fn test_capability_set_basic_operations() {
        let mut caps = CapabilitySet::new();
        assert!(caps.is_empty());

        caps.add(Capability::Read);
        assert!(!caps.is_empty());
        assert!(caps.has(Capability::Read));
        assert!(!caps.has(Capability::Submit));

        caps.remove(Capability::Read);
        assert!(!caps.has(Capability::Read));
    }

    #[test]
    fn test_capability_set_from_vec() {
        let caps: CapabilitySet = vec![Capability::Read, Capability::Submit].into();
        assert!(caps.has(Capability::Read));
        assert!(caps.has(Capability::Submit));
        assert!(!caps.has(Capability::Fork));
    }

    #[test]
    fn test_capability_serialization() {
        let cap = Capability::Delegate;
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"delegate\"");

        let deserialized: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Capability::Delegate);
    }

    #[test]
    fn test_capability_set_to_vec() {
        let mut caps = CapabilitySet::new();
        caps.add(Capability::Read);
        caps.add(Capability::Submit);
        let vec = caps.to_vec();
        assert_eq!(vec.len(), 2);
        assert!(vec.contains(&Capability::Read));
        assert!(vec.contains(&Capability::Submit));
    }
}
