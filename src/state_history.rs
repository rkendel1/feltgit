// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation: version 2 of the License, dated June 1991.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, see <https://www.gnu.org/licenses/>.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

/// An error indicating an invalid state history operation.
#[derive(Debug, Clone)]
pub enum StateHistoryError {
    InvalidStateIdentity,
    MissingParent,
    InvalidAuthority,
    DuplicateRevision,
    SerializationError(String),
    DeserializationError(String),
    IoError(String),
    PersistenceError(String),
}

impl Display for StateHistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateHistoryError::InvalidStateIdentity => write!(f, "invalid state identity"),
            StateHistoryError::MissingParent => write!(f, "missing parent"),
            StateHistoryError::InvalidAuthority => write!(f, "invalid authority"),
            StateHistoryError::DuplicateRevision => write!(f, "duplicate revision"),
            StateHistoryError::SerializationError(e) => write!(f, "serialization error: {}", e),
            StateHistoryError::DeserializationError(e) => write!(f, "deserialization error: {}", e),
            StateHistoryError::IoError(e) => write!(f, "io error: {}", e),
            StateHistoryError::PersistenceError(e) => write!(f, "persistence error: {}", e),
        }
    }
}

impl Error for StateHistoryError {}

/// A deterministic hash-based state identifier.
/// Content-addressed: same canonical state always produces same StateId.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct StateId {
    hash: [u8; 32],
}

impl StateId {
    /// Create a StateId from a 32-byte hash.
    pub fn new(hash: [u8; 32]) -> Self {
        Self { hash }
    }

    /// Get the hash as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.hash
    }

    /// Convert to hex string for display and serialization.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.hash)
    }

    /// Parse from hex string.
    pub fn from_hex(hex_str: &str) -> Result<Self, StateHistoryError> {
        if hex_str.len() != 64 {
            return Err(StateHistoryError::InvalidStateIdentity);
        }
        let bytes = hex::decode(hex_str)
            .map_err(|_| StateHistoryError::InvalidStateIdentity)?;
        if bytes.len() != 32 {
            return Err(StateHistoryError::InvalidStateIdentity);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Self { hash })
    }
}

impl Display for StateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// An explicit authority identity.
/// Used to represent "This revision was authored under authority X."
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AuthorityId {
    id: String,
}

impl AuthorityId {
    /// Create a new AuthorityId.
    /// The id must be non-empty and valid UTF-8.
    pub fn new(id: impl Into<String>) -> Result<Self, StateHistoryError> {
        let id = id.into();
        if id.is_empty() {
            return Err(StateHistoryError::InvalidAuthority);
        }
        Ok(Self { id })
    }

    /// Get the authority id as a string.
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

impl Display for AuthorityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// A canonical state representation that ensures deterministic state identity.
/// For JSON state, this canonicalizes key ordering.
pub struct CanonicalState {
    canonical_json: String,
}

impl CanonicalState {
    /// Create a canonical state from JSON state.
    /// Ensures consistent key ordering for deterministic hashing.
    pub fn from_json(state: &Value) -> Result<Self, StateHistoryError> {
        let canonical_json = Self::canonicalize_value(state)?;
        Ok(Self { canonical_json })
    }

    /// Create a canonical state from a JSON string.
    pub fn from_json_str(json_str: &str) -> Result<Self, StateHistoryError> {
        let value: Value = serde_json::from_str(json_str)
            .map_err(|e| StateHistoryError::DeserializationError(e.to_string()))?;
        Self::from_json(&value)
    }

    /// Get the canonical JSON string.
    pub fn as_str(&self) -> &str {
        &self.canonical_json
    }

    /// Get the bytes of the canonical JSON.
    pub fn as_bytes(&self) -> &[u8] {
        self.canonical_json.as_bytes()
    }

    /// Canonicalize a JSON value by sorting object keys recursively.
    ///
    /// CONTRACT: Numeric representations are preserved as-is by serde_json serialization.
    /// That is:
    /// - json!(1) serializes as "1"
    /// - json!(1.0) serializes as "1.0"
    /// - json!(1e0) serializes as "1e0"
    /// These produce different canonical JSON strings and thus different StateIds.
    /// This is a representation-sensitive contract that preserves information.
    ///
    /// Type distinctions are always preserved:
    /// - false vs null vs 0 vs "" all produce distinct StateIds
    /// - [] vs {} are distinct
    /// - true vs 1 are distinct
    ///
    /// This is the minimal deterministic contract without semantic normalization.
    fn canonicalize_value(value: &Value) -> Result<String, StateHistoryError> {
        match value {
            Value::Object(map) => {
                let mut sorted_map = Map::new();
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort();
                for key in keys {
                    if let Some(val) = map.get(&key) {
                        sorted_map.insert(
                            key,
                            serde_json::from_str(&Self::canonicalize_value(val)?)
                                .map_err(|e| StateHistoryError::SerializationError(e.to_string()))?,
                        );
                    }
                }
                serde_json::to_string(&Value::Object(sorted_map))
                    .map_err(|e| StateHistoryError::SerializationError(e.to_string()))
            }
            Value::Array(arr) => {
                let canonicalized: Result<Vec<String>, _> =
                    arr.iter().map(Self::canonicalize_value).collect();
                let json_array: Result<Vec<Value>, _> = canonicalized?
                    .iter()
                    .map(|s| serde_json::from_str(s))
                    .collect();
                serde_json::to_string(&json_array.map_err(|e| {
                    StateHistoryError::SerializationError(e.to_string())
                })?)
                .map_err(|e| StateHistoryError::SerializationError(e.to_string()))
            }
            _ => serde_json::to_string(value)
                .map_err(|e| StateHistoryError::SerializationError(e.to_string())),
        }
    }
}

/// Calculate deterministic state identity from canonical state.
pub fn calculate_state_id(canonical_state: &CanonicalState) -> StateId {
    let mut hasher = Sha256::new();
    hasher.update(canonical_state.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    StateId::new(hash)
}

/// A durable application-state revision with explicit causal ancestry and authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateRevision {
    /// Deterministic content-addressed state identifier.
    pub state_id: StateId,

    /// The immediate causal predecessor, if any.
    pub parent: Option<StateId>,

    /// Explicit authority identity under which this revision was authored.
    pub authority: AuthorityId,

    /// The actual state content (canonical JSON).
    pub state: String,
}

impl StateRevision {
    /// Create a new state revision.
    /// Validates that:
    /// - state_id matches the canonical state
    /// - authority is valid
    /// - if parent is Some, it represents a valid reference
    pub fn new(
        state: &Value,
        parent: Option<StateId>,
        authority: AuthorityId,
    ) -> Result<Self, StateHistoryError> {
        let canonical = CanonicalState::from_json(state)?;
        let calculated_id = calculate_state_id(&canonical);

        Ok(StateRevision {
            state_id: calculated_id,
            parent,
            authority,
            state: canonical.as_str().to_string(),
        })
    }

    /// Create a new state revision from a JSON string.
    pub fn from_json_str(
        json_str: &str,
        parent: Option<StateId>,
        authority: AuthorityId,
    ) -> Result<Self, StateHistoryError> {
        let value: Value = serde_json::from_str(json_str)
            .map_err(|e| StateHistoryError::DeserializationError(e.to_string()))?;
        Self::new(&value, parent, authority)
    }

    /// Verify that the revision's state matches its state_id.
    pub fn verify(&self) -> Result<(), StateHistoryError> {
        let canonical = CanonicalState::from_json_str(&self.state)?;
        let calculated_id = calculate_state_id(&canonical);

        if calculated_id == self.state_id {
            Ok(())
        } else {
            Err(StateHistoryError::InvalidStateIdentity)
        }
    }

    /// Get the state as a JSON Value.
    pub fn state_json(&self) -> Result<Value, StateHistoryError> {
        serde_json::from_str(&self.state)
            .map_err(|e| StateHistoryError::DeserializationError(e.to_string()))
    }
}

/// Describes the causal relationship between two state revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRelationship {
    /// The two states are identical (same StateId).
    Identity,
    /// The left state is an ancestor of the right state.
    Ancestor,
    /// The left state is a descendant of the right state.
    Descendant,
    /// The states diverged from a common ancestor but neither is an ancestor of the other.
    Diverged,
    /// The states have no causal relationship (no common ancestor in the history).
    Unrelated,
}

impl Display for StateRelationship {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateRelationship::Identity => write!(f, "Identity"),
            StateRelationship::Ancestor => write!(f, "Ancestor"),
            StateRelationship::Descendant => write!(f, "Descendant"),
            StateRelationship::Diverged => write!(f, "Diverged"),
            StateRelationship::Unrelated => write!(f, "Unrelated"),
        }
    }
}

/// A durable state history storage.
/// Manages persistence and retrieval of state revisions.
pub struct StateHistory {
    storage_dir: PathBuf,
    revisions: BTreeMap<StateId, StateRevision>,
    authority: AuthorityId,
}

impl StateHistory {
    /// Create a new state history with the given storage directory and authority.
    pub fn new(storage_dir: impl AsRef<Path>, authority: AuthorityId) -> Result<Self, StateHistoryError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();

        // Ensure storage directory exists
        std::fs::create_dir_all(&storage_dir)
            .map_err(|e| StateHistoryError::IoError(e.to_string()))?;

        let mut history = StateHistory {
            storage_dir,
            revisions: BTreeMap::new(),
            authority,
        };

        // Load existing revisions from disk
        history.load_all_revisions()?;

        Ok(history)
    }

    /// Create a new state revision under this authority.
    /// Returns the persisted revision.
    pub fn create_revision(
        &mut self,
        state: &Value,
        parent: Option<StateId>,
    ) -> Result<StateRevision, StateHistoryError> {
        // Validate parent exists if provided
        if let Some(parent_id) = parent {
            if !self.revisions.contains_key(&parent_id) {
                return Err(StateHistoryError::MissingParent);
            }
        }

        let revision = StateRevision::new(state, parent, self.authority.clone())?;

        // Check for duplicate (idempotent)
        if let Some(existing) = self.revisions.get(&revision.state_id) {
            if existing == &revision {
                return Ok(revision);
            }
            return Err(StateHistoryError::DuplicateRevision);
        }

        // Persist to storage
        self.persist_revision(&revision)?;

        // Store in memory map
        self.revisions.insert(revision.state_id, revision.clone());

        Ok(revision)
    }

    /// Load a revision by its state_id.
    pub fn load_revision(&self, state_id: StateId) -> Result<StateRevision, StateHistoryError> {
        self.revisions
            .get(&state_id)
            .cloned()
            .ok_or(StateHistoryError::PersistenceError(
                "revision not found".to_string(),
            ))
    }

    /// Get all revisions in order of creation.
    pub fn all_revisions(&self) -> Vec<StateRevision> {
        self.revisions.values().cloned().collect()
    }

    /// Persist a revision to disk.
    fn persist_revision(&self, revision: &StateRevision) -> Result<(), StateHistoryError> {
        let state_id_hex = revision.state_id.to_hex();
        let revision_path = self.storage_dir.join(&state_id_hex);

        let serialized = serde_json::to_string_pretty(revision)
            .map_err(|e| StateHistoryError::SerializationError(e.to_string()))?;

        std::fs::write(&revision_path, serialized)
            .map_err(|e| StateHistoryError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Load all revisions from disk.
    fn load_all_revisions(&mut self) -> Result<(), StateHistoryError> {
        let entries = std::fs::read_dir(&self.storage_dir)
            .map_err(|e| StateHistoryError::IoError(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| StateHistoryError::IoError(e.to_string()))?;
            let path = entry.path();

            if path.is_file() {
                let contents = std::fs::read_to_string(&path)
                    .map_err(|e| StateHistoryError::IoError(e.to_string()))?;

                let revision: StateRevision = serde_json::from_str(&contents)
                    .map_err(|e| StateHistoryError::DeserializationError(e.to_string()))?;

                // Verify integrity
                revision.verify()?;

                self.revisions.insert(revision.state_id, revision);
            }
        }

        Ok(())
    }

    /// Get all ancestors of a state, ordered from immediate parent to root.
    /// Returns an error if the state is not found.
    /// Returns an empty vector if the state is a root (has no parents).
    pub fn ancestors(&self, state_id: StateId) -> Result<Vec<StateId>, StateHistoryError> {
       let mut ancestors = Vec::new();
       let mut current = state_id;

       loop {
           let revision = self.revisions
               .get(&current)
               .ok_or(StateHistoryError::PersistenceError(
                   "revision not found".to_string(),
               ))?;

           match revision.parent {
               Some(parent_id) => {
                   ancestors.push(parent_id);
                   current = parent_id;
               }
               None => break,
           }
       }

       Ok(ancestors)
    }

    /// Check if one state is an ancestor of another.
    /// Returns false if either state does not exist.
    pub fn is_ancestor(&self, ancestor: StateId, descendant: StateId) -> bool {
       if ancestor == descendant {
           return false;
       }

       match self.ancestors(descendant) {
           Ok(ancestors) => ancestors.iter().any(|&a| a == ancestor),
           Err(_) => false,
       }
    }

    /// Find the most recent common ancestor of two states.
    /// Returns None if the states have no common ancestor or if either state doesn't exist.
    pub fn common_ancestor(&self, left: StateId, right: StateId) -> Option<StateId> {
       if left == right {
           return Some(left);
       }

       // Check if left is an ancestor of right
       if self.is_ancestor(left, right) {
           return Some(left);
       }

       // Check if right is an ancestor of left
       if self.is_ancestor(right, left) {
           return Some(right);
       }

       // Get all ancestors for both states
       let left_ancestors = match self.ancestors(left) {
           Ok(ancestors) => ancestors,
           Err(_) => return None,
       };

       let right_ancestors = match self.ancestors(right) {
           Ok(ancestors) => ancestors,
           Err(_) => return None,
       };

       // Find the most recent (earliest in the list) common ancestor
       for &left_ancestor in &left_ancestors {
           if right_ancestors.iter().any(|&r| r == left_ancestor) {
               return Some(left_ancestor);
           }
       }

       None
    }

    /// Determine the causal relationship between two state revisions.
    /// Returns an error if either state does not exist.
    pub fn relationship(&self, left: StateId, right: StateId) -> Result<StateRelationship, StateHistoryError> {
       // Verify both states exist first
       let _ = self.load_revision(left)?;
       let _ = self.load_revision(right)?;

       // Check if they're the same
       if left == right {
           return Ok(StateRelationship::Identity);
       }

       // Check if left is an ancestor of right
       if self.is_ancestor(left, right) {
           return Ok(StateRelationship::Ancestor);
       }

       // Check if left is a descendant of right
       if self.is_ancestor(right, left) {
           return Ok(StateRelationship::Descendant);
       }

       // Check if they have a common ancestor (diverged)
       if self.common_ancestor(left, right).is_some() {
           return Ok(StateRelationship::Diverged);
       }

       // No relationship
       Ok(StateRelationship::Unrelated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_state_id_deterministic() {
        let state = json!({"name": "Randy", "role": "admin"});
        let canonical1 = CanonicalState::from_json(&state).unwrap();
        let id1 = calculate_state_id(&canonical1);

        let canonical2 = CanonicalState::from_json(&state).unwrap();
        let id2 = calculate_state_id(&canonical2);

        assert_eq!(id1, id2, "Same state should produce same identity");
    }

    #[test]
    fn test_state_id_different_content() {
        let state1 = json!({"name": "Randy", "role": "admin"});
        let state2 = json!({"name": "Alice", "role": "user"});

        let canonical1 = CanonicalState::from_json(&state1).unwrap();
        let id1 = calculate_state_id(&canonical1);

        let canonical2 = CanonicalState::from_json(&state2).unwrap();
        let id2 = calculate_state_id(&canonical2);

        assert_ne!(id1, id2, "Different states should produce different identities");
    }

    #[test]
    fn test_json_key_ordering_same_identity() {
        let state1 = json!({"name": "Randy", "role": "admin"});
        let state2 = json!({"role": "admin", "name": "Randy"});

        let canonical1 = CanonicalState::from_json(&state1).unwrap();
        let id1 = calculate_state_id(&canonical1);

        let canonical2 = CanonicalState::from_json(&state2).unwrap();
        let id2 = calculate_state_id(&canonical2);

        assert_eq!(
            id1, id2,
            "Different key ordering should produce same identity"
        );
    }

    #[test]
    fn test_state_revision_creation() {
        let state = json!({"name": "Randy", "role": "admin"});
        let authority = AuthorityId::new("alice").unwrap();

        let revision = StateRevision::new(&state, None, authority).unwrap();

        assert_eq!(revision.parent, None, "Root revision should have no parent");
        assert_eq!(revision.authority.as_str(), "alice");
        revision.verify().unwrap();
    }

    #[test]
    fn test_state_revision_with_parent() {
        let authority = AuthorityId::new("bob").unwrap();

        let state1 = json!({"version": 1});
        let rev1 = StateRevision::new(&state1, None, authority.clone()).unwrap();

        let state2 = json!({"version": 2});
        let rev2 = StateRevision::new(&state2, Some(rev1.state_id), authority).unwrap();

        assert_eq!(rev2.parent, Some(rev1.state_id));
        rev2.verify().unwrap();
    }

    #[test]
    fn test_state_revision_invalid_authority() {
        let result = AuthorityId::new("");
        assert!(result.is_err(), "Empty authority should be invalid");
    }

    #[test]
    fn test_state_id_hex_round_trip() {
        let state = json!({"key": "value"});
        let canonical = CanonicalState::from_json(&state).unwrap();
        let id1 = calculate_state_id(&canonical);

        let hex = id1.to_hex();
        let id2 = StateId::from_hex(&hex).unwrap();

        assert_eq!(id1, id2, "State ID should round-trip through hex");
    }

    #[test]
    fn test_persistence_write_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("charlie").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let state = json!({"data": "test"});
        let revision = history.create_revision(&state, None).unwrap();

        let loaded = history.load_revision(revision.state_id).unwrap();
        assert_eq!(loaded, revision);
    }

    #[test]
    fn test_persistence_restart_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("diana").unwrap();

        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"persistent": true});
            let _revision = history.create_revision(&state, None).unwrap();
        }

        // Simulate restart by creating new history instance
        let history2 = StateHistory::new(temp_dir.path(), authority).unwrap();
        let all_revisions = history2.all_revisions();

        assert_eq!(
            all_revisions.len(),
            1,
            "Should reload revision after restart"
        );
        assert_eq!(
            all_revisions[0].state,
            r#"{"persistent":true}"#,
            "State content should be preserved"
        );
    }

    #[test]
    fn test_multi_step_history_restart() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("eve").unwrap();

        let (state1_id, state2_id) = {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();

            let state1 = json!({"step": 1});
            let rev1 = history.create_revision(&state1, None).unwrap();

            let state2 = json!({"step": 2});
            let rev2 = history
                .create_revision(&state2, Some(rev1.state_id))
                .unwrap();

            (rev1.state_id, rev2.state_id)
        };

        // Restart and verify
        let history2 = StateHistory::new(temp_dir.path(), authority).unwrap();
        let _rev1_loaded = history2.load_revision(state1_id).unwrap();
        let rev2_loaded = history2.load_revision(state2_id).unwrap();

        assert_eq!(rev2_loaded.parent, Some(state1_id));
    }

    #[test]
    fn test_authority_persisted() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("frank").unwrap();

        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"authority_test": true});
            let _revision = history.create_revision(&state, None).unwrap();
        }

        let history2 = StateHistory::new(temp_dir.path(), authority).unwrap();
        let revisions = history2.all_revisions();
        assert_eq!(revisions[0].authority.as_str(), "frank");
    }

    #[test]
    fn test_same_state_different_authority_distinct() {
        let temp_dir = TempDir::new().unwrap();
        let state = json!({"same": "content"});

        let auth1 = AuthorityId::new("grace").unwrap();
        let auth2 = AuthorityId::new("henry").unwrap();

        let mut hist1 = StateHistory::new(temp_dir.path().join("hist1"), auth1).unwrap();
        let mut hist2 = StateHistory::new(temp_dir.path().join("hist2"), auth2).unwrap();

        let rev1 = hist1.create_revision(&state, None).unwrap();
        let rev2 = hist2.create_revision(&state, None).unwrap();

        // Same state content = same state_id
        assert_eq!(rev1.state_id, rev2.state_id);
        // But different authority
        assert_ne!(rev1.authority, rev2.authority);
    }

    #[test]
    fn test_missing_parent_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("iris").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let fake_parent = StateId::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();

        let state = json!({"test": "data"});
        let result = history.create_revision(&state, Some(fake_parent));

        assert!(result.is_err(), "Should reject missing parent");
    }

    #[test]
    fn test_invalid_state_identity_rejected() {
        let authority = AuthorityId::new("jack").unwrap();

        let state = json!({"key": "value"});
        let mut revision = StateRevision::new(&state, None, authority).unwrap();

        // Corrupt the state_id
        revision.state_id = StateId::new([0u8; 32]);

        let result = revision.verify();
        assert!(result.is_err(), "Should reject invalid state identity");
    }

    #[test]
    fn test_duplicate_revision_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("kelly").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let state = json!({"idempotent": true});
        let rev1 = history.create_revision(&state, None).unwrap();
        let rev2 = history.create_revision(&state, None).unwrap();

        // Both should have same state_id
        assert_eq!(rev1.state_id, rev2.state_id);
    }

    #[test]
    fn test_immutability_no_silent_mutation() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("leo").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let state1 = json!({"version": 1});
        let rev1 = history.create_revision(&state1, None).unwrap();

        let state2 = json!({"version": 2});
        let rev2 = history.create_revision(&state2, None).unwrap();

        // Different states = different identities
        assert_ne!(rev1.state_id, rev2.state_id);

        // Original state_id should still load original state
        let loaded = history.load_revision(rev1.state_id).unwrap();
        assert_eq!(loaded.state, rev1.state);
    }

    #[test]
    fn test_no_reconciliation_occurs() {
        let temp_dir = TempDir::new().unwrap();
        let authority1 = AuthorityId::new("mia").unwrap();
        let authority2 = AuthorityId::new("noah").unwrap();

        let mut hist1 = StateHistory::new(temp_dir.path().join("hist1"), authority1).unwrap();
        let mut hist2 = StateHistory::new(temp_dir.path().join("hist2"), authority2).unwrap();

        let state1 = json!({"from": "mia"});
        let state2 = json!({"from": "noah"});

        let rev1 = hist1.create_revision(&state1, None).unwrap();
        let rev2 = hist2.create_revision(&state2, None).unwrap();

        // Creating revisions should not cause reconciliation
        // (verified by successful creation)
        assert_ne!(rev1.state_id, rev2.state_id);
    }

    #[test]
    fn test_canonical_json_nested_objects() {
        let state1 = json!({
            "person": {
                "name": "Randy",
                "age": 30
            },
            "role": "admin"
        });

        let state2 = json!({
            "role": "admin",
            "person": {
                "age": 30,
                "name": "Randy"
            }
        });

        let canonical1 = CanonicalState::from_json(&state1).unwrap();
        let id1 = calculate_state_id(&canonical1);

        let canonical2 = CanonicalState::from_json(&state2).unwrap();
        let id2 = calculate_state_id(&canonical2);

        assert_eq!(id1, id2, "Nested objects with different key order should match");
    }

    #[test]
    fn test_state_revision_verification() {
        let state = json!({"verify": "me"});
        let authority = AuthorityId::new("oscar").unwrap();

        let revision = StateRevision::new(&state, None, authority).unwrap();
        let result = revision.verify();

        assert!(result.is_ok(), "Valid revision should verify");
    }

    #[test]
    fn test_authority_independent_from_state_id() {
        let authority1 = AuthorityId::new("pam").unwrap();
        let authority2 = AuthorityId::new("quentin").unwrap();

        let state = json!({"data": "same"});

        let rev1 = StateRevision::new(&state, None, authority1).unwrap();
        let rev2 = StateRevision::new(&state, None, authority2).unwrap();

        // Same state must produce same state_id regardless of authority
        assert_eq!(rev1.state_id, rev2.state_id);
        // But authority is different
        assert_ne!(rev1.authority, rev2.authority);
    }

    #[test]
    fn test_all_revisions_order() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("rachel").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let state1 = json!({"id": 1});
        let rev1 = history.create_revision(&state1, None).unwrap();

        let state2 = json!({"id": 2});
        let _rev2 = history.create_revision(&state2, Some(rev1.state_id)).unwrap();

        let all = history.all_revisions();
        assert_eq!(all.len(), 2);
    }

    // ============================================
    // GATE 1: CANONICAL IDENTITY - CRITICAL TESTS
    // ============================================

    #[test]
    fn test_json_number_representation_sensitivity() {
        // AUDIT GATE 1: CANONICALIZATION CONTRACT
        // This test documents the canonical state identity contract:
        // JSON numeric representations are preserved as-is by serde_json.
        // Therefore, different JSON representations produce DIFFERENT StateIds.
        // This is a representation-sensitive (not semantic-normalized) contract.
        
        let state1 = json!({"value": 1});
        let state2 = json!({"value": 1.0});
        
        let canonical1 = CanonicalState::from_json(&state1).unwrap();
        let canonical2 = CanonicalState::from_json(&state2).unwrap();
        
        // serde_json preserves JSON representation: 1 vs 1.0
        // Therefore canonical JSON strings are different
        assert_ne!(
            canonical1.as_str(),
            canonical2.as_str(),
            "JSON representations 1 and 1.0 must have different canonical forms"
        );
        
        let id1 = calculate_state_id(&canonical1);
        let id2 = calculate_state_id(&canonical2);
        
        // Different canonical strings → different hashes → different state_ids
        assert_ne!(
            id1, id2,
            "1 and 1.0 produce different state_ids per representation-sensitive contract"
        );
    }

    #[test]
    fn test_json_deterministic_repeated_calculation() {
        // AUDIT GATE 1: Determinism within the chosen contract
        // Same JSON representation must always produce identical StateId
        let state = json!({"value": 1.0});
        
        let canonical1 = CanonicalState::from_json(&state).unwrap();
        let canonical2 = CanonicalState::from_json(&state).unwrap();
        
        // Same representation must produce identical bytes
        assert_eq!(
            canonical1.as_bytes(),
            canonical2.as_bytes(),
            "Repeated canonicalization must produce byte-identical output"
        );
        
        let id1 = calculate_state_id(&canonical1);
        let id2 = calculate_state_id(&canonical2);
        
        // Same input → identical output (determinism)
        assert_eq!(
            id1, id2,
            "Repeated state_id calculation must be deterministic for same input"
        );
    }

    #[test]
    fn test_json_object_key_ordering_irrelevant() {
        // AUDIT GATE 1: Key ordering must not affect state identity
        let state1 = json!({"name": "Randy", "role": "admin"});
        let state2 = json!({"role": "admin", "name": "Randy"});
        
        let canonical1 = CanonicalState::from_json(&state1).unwrap();
        let canonical2 = CanonicalState::from_json(&state2).unwrap();
        
        // Both must canonicalize to the same form (sorted keys)
        assert_eq!(
            canonical1.as_str(),
            canonical2.as_str(),
            "Object key order must not affect canonical form"
        );
        
        let id1 = calculate_state_id(&canonical1);
        let id2 = calculate_state_id(&canonical2);
        
        assert_eq!(
            id1, id2,
            "Different key ordering must produce identical state_id"
        );
    }

    // =====================================================
    // GATE 2: TYPE/REPRESENTATION SAFETY - CRITICAL TESTS
    // =====================================================

    #[test]
    fn test_type_safety_false_vs_null_vs_zero() {
        // AUDIT GATE 2: Can we distinguish false, null, 0, "" ?
        // Risk: Falsey values become indistinguishable
        let state_false = json!({"value": false});
        let state_null = json!({"value": null});
        let state_zero = json!({"value": 0});
        let state_empty = json!({"value": ""});
        
        let canonical_false = CanonicalState::from_json(&state_false).unwrap();
        let canonical_null = CanonicalState::from_json(&state_null).unwrap();
        let canonical_zero = CanonicalState::from_json(&state_zero).unwrap();
        let canonical_empty = CanonicalState::from_json(&state_empty).unwrap();
        
        let id_false = calculate_state_id(&canonical_false);
        let id_null = calculate_state_id(&canonical_null);
        let id_zero = calculate_state_id(&canonical_zero);
        let id_empty = calculate_state_id(&canonical_empty);
        
        // All must be DISTINCT
        assert_ne!(id_false, id_null, "false and null must have different state_ids");
        assert_ne!(id_false, id_zero, "false and 0 must have different state_ids");
        assert_ne!(id_false, id_empty, "false and \"\" must have different state_ids");
        assert_ne!(id_null, id_zero, "null and 0 must have different state_ids");
        assert_ne!(id_null, id_empty, "null and \"\" must have different state_ids");
        assert_ne!(id_zero, id_empty, "0 and \"\" must have different state_ids");
    }

    #[test]
    fn test_type_safety_boolean_true_vs_one() {
        // AUDIT GATE 2: true vs 1 must be distinct
        let state_true = json!({"value": true});
        let state_one = json!({"value": 1});
        
        let canonical_true = CanonicalState::from_json(&state_true).unwrap();
        let canonical_one = CanonicalState::from_json(&state_one).unwrap();
        
        let id_true = calculate_state_id(&canonical_true);
        let id_one = calculate_state_id(&canonical_one);
        
        assert_ne!(id_true, id_one, "true and 1 must have different state_ids");
    }

    #[test]
    fn test_type_safety_empty_array_vs_empty_object() {
        // AUDIT GATE 2: [] vs {} must be distinct
        let state_array = json!({"data": []});
        let state_object = json!({"data": {}});
        
        let canonical_array = CanonicalState::from_json(&state_array).unwrap();
        let canonical_object = CanonicalState::from_json(&state_object).unwrap();
        
        let id_array = calculate_state_id(&canonical_array);
        let id_object = calculate_state_id(&canonical_object);
        
        assert_ne!(id_array, id_object, "[] and {{}} must have different state_ids");
    }

    #[test]
    fn test_type_safety_string_vs_number_strings() {
        // AUDIT GATE 2: "123" vs 123 must be distinct
        let state_string = json!({"value": "123"});
        let state_number = json!({"value": 123});
        
        let canonical_string = CanonicalState::from_json(&state_string).unwrap();
        let canonical_number = CanonicalState::from_json(&state_number).unwrap();
        
        let id_string = calculate_state_id(&canonical_string);
        let id_number = calculate_state_id(&canonical_number);
        
        assert_ne!(id_string, id_number, "\"123\" and 123 must have different state_ids");
    }

    // ===============================================
    // GATE 5: DURABILITY - CORRUPTION DETECTION TESTS
    // ===============================================

    #[test]
    fn test_corrupted_json_file_rejected() {
        // AUDIT GATE 5: Verify corrupted persisted records fail explicitly
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("simon").unwrap();
        
        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"data": "test"});
            let _revision = history.create_revision(&state, None).unwrap();
        }
        
        // Corrupt the persisted file
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        
        assert_eq!(entries.len(), 1, "Should have one persisted file");
        let file_path = entries[0].path();
        
        // Write invalid JSON
        std::fs::write(&file_path, "{ invalid json").unwrap();
        
        // Attempt to reload should fail explicitly
        let result = StateHistory::new(temp_dir.path(), authority);
        assert!(result.is_err(), "Should reject corrupted JSON file");
        
        match result {
            Err(StateHistoryError::DeserializationError(_)) => {
                // Correct error type
            }
            Err(e) => {
                panic!("Expected DeserializationError, got: {:?}", e);
            }
            Ok(_) => {
                panic!("Should reject corrupted JSON file");
            }
        }
    }

    #[test]
    fn test_missing_state_id_field_rejected() {
        // AUDIT GATE 5: Missing required fields must be detected
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("tara").unwrap();
        
        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"data": "test"});
            let _revision = history.create_revision(&state, None).unwrap();
        }
        
        // Corrupt by removing state_id field
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        
        let file_path = entries[0].path();
        let contents = std::fs::read_to_string(&file_path).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&contents).unwrap();
        json.as_object_mut().unwrap().remove("state_id");
        
        std::fs::write(&file_path, json.to_string()).unwrap();
        
        // Reload should fail
        let result = StateHistory::new(temp_dir.path(), authority);
        assert!(result.is_err(), "Should reject revision with missing state_id");
    }

    #[test]
    fn test_invalid_state_id_hex_rejected() {
        // AUDIT GATE 5: Invalid hex in state_id must be detected
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("uma").unwrap();
        
        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"data": "test"});
            let _revision = history.create_revision(&state, None).unwrap();
        }
        
        // Corrupt hex in state_id
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        
        let file_path = entries[0].path();
        let contents = std::fs::read_to_string(&file_path).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&contents).unwrap();
        
        // Set state_id to invalid hex
        json["state_id"] = json!("ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ");
        
        std::fs::write(&file_path, json.to_string()).unwrap();
        
        // Reload should fail (deserialization or verification)
        let result = StateHistory::new(temp_dir.path(), authority);
        assert!(result.is_err(), "Should reject invalid state_id hex format");
    }

    // ====================================================
    // GATE 6: DUPLICATE/IDEMPOTENCY SEMANTICS - MORE TESTS
    // ====================================================

    #[test]
    fn test_duplicate_with_different_parent_error() {
        // AUDIT GATE 6: Partial duplicate detection
        // Same state_id, different parent should error (can't have 2 parents)
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("victor").unwrap();
        
        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();
        
        // Create two root revisions with different states
        let state_a = json!({"id": "a"});
        let rev_a = history.create_revision(&state_a, None).unwrap();
        
        let state_b = json!({"id": "b"});
        let rev_b = history.create_revision(&state_b, None).unwrap();
        
        // Create a child of rev_a
        let state_child = json!({"parent": "a"});
        let rev_child = history.create_revision(&state_child, Some(rev_a.state_id)).unwrap();
        
        // Try to create identical child with different parent (rev_b)
        // This should error because state_id already exists with different parent
        let result = history.create_revision(&state_child, Some(rev_b.state_id));
        
        assert!(result.is_err(), "Cannot create same state_id with different parent");
        match result {
            Err(StateHistoryError::DuplicateRevision) => {
                // Expected behavior
            }
            _ => {
                panic!("Expected DuplicateRevision error");
            }
        }
    }

    #[test]
    fn test_exact_duplicate_idempotent() {
        // AUDIT GATE 6: Exact duplicate (state + parent + authority) must be idempotent
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("wendy").unwrap();
        
        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();
        
        let state = json!({"idempotent": true});
        let rev1 = history.create_revision(&state, None).unwrap();
        
        // Attempt identical creation again
        let rev2 = history.create_revision(&state, None).unwrap();
        
        // Must succeed and return same state_id
        assert_eq!(rev1.state_id, rev2.state_id, "Duplicate must be idempotent");
        assert_eq!(rev1, rev2, "Duplicate revisions must be identical");
    }

    // ============================================================
    // GATE 7: STORAGE INVARIANTS - PARENT REFERENCE INTEGRITY TESTS
    // ============================================================

    #[test]
    fn test_parent_reference_persisted_correctly() {
        // AUDIT GATE 7: Verify parent references are stored and retrieved correctly
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("xavier").unwrap();
        
        let parent_id = {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state1 = json!({"step": 1});
            let rev1 = history.create_revision(&state1, None).unwrap();
            rev1.state_id
        };
        
        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state2 = json!({"step": 2});
            let _rev2 = history.create_revision(&state2, Some(parent_id)).unwrap();
        }
        
        // Close and reopen
        let history3 = StateHistory::new(temp_dir.path(), authority).unwrap();
        let all_revs = history3.all_revisions();
        
        assert_eq!(all_revs.len(), 2, "Both revisions should be loaded");
        
        let rev2 = all_revs.iter().find(|r| r.parent.is_some()).unwrap();
        assert_eq!(
            rev2.parent,
            Some(parent_id),
            "Parent reference must be persisted and restored exactly"
        );
    }

    #[test]
    fn test_storage_file_format_contains_all_fields() {
        // AUDIT GATE 7: Inspect on-disk format to verify completeness
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("yolanda").unwrap();
        
        {
            let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();
            let state = json!({"format": "check"});
            let revision = history.create_revision(&state, None).unwrap();
            
            // Save state_id for verification
            let expected_state_id = revision.state_id.to_hex();
            
            // Verify file was created with correct name
            let file_path = temp_dir.path().join(&expected_state_id);
            assert!(file_path.exists(), "File should exist with state_id as hex name");
            
            // Read and verify content
            let contents = std::fs::read_to_string(&file_path).unwrap();
            let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
            
            // Verify all required fields are persisted
            assert!(json.get("state_id").is_some(), "state_id must be in persisted file");
            assert!(json.get("authority").is_some(), "authority must be in persisted file");
            assert!(json.get("state").is_some(), "state must be in persisted file");
            assert!(json.get("parent").is_some(), "parent must be in persisted file");
            
            // Verify state is the correct canonical form
            let persisted_state = json["state"].as_str().unwrap();
            assert_eq!(
                persisted_state,
                r#"{"format":"check"}"#,
                "Persisted state must be canonical JSON"
            );
        }
    }

    // ================================================
    // GATE 3: REVISION SEMANTICS - ORPHAN DETECTION
    // ================================================

    #[test]
    fn test_orphan_detection_parent_deleted() {
        // AUDIT GATE 3: Can we detect orphaned revisions?
        // Note: Current implementation doesn't prevent this at creation time
        // This test documents the current behavior
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("zoe").unwrap();
        
        let parent_id = {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"step": 1});
            let rev = history.create_revision(&state, None).unwrap();
            rev.state_id
        };
        
        // Create child that references parent
        {
            let mut history = StateHistory::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"step": 2});
            let _rev = history.create_revision(&state, Some(parent_id)).unwrap();
        }
        
        // Try to manually delete the parent file and reload
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        
        // Find and remove parent file
        for entry in entries {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            if file_name == parent_id.to_hex() {
                std::fs::remove_file(&path).unwrap();
            }
        }
        
        // Reload - child still exists but references missing parent
        let history_reloaded = StateHistory::new(temp_dir.path(), authority).unwrap();
        let all_revs = history_reloaded.all_revisions();
        
        let child_rev = all_revs.iter().find(|r| r.parent.is_some()).unwrap();
        
        // Verify parent doesn't exist in storage
        let parent_exists = all_revs.iter().any(|r| r.state_id == parent_id);
        assert!(!parent_exists, "Parent should not exist after deletion");
        assert_eq!(
            child_rev.parent,
            Some(parent_id),
            "Child still references deleted parent (orphan)"
        );
        
        // Document: Current behavior allows orphans (future PR should add validation)
    }

    // ========================================
    // GATE 9: NEGATIVE TESTS - ERROR PATHS
    // ========================================

    #[test]
    fn test_invalid_authority_format_rejected() {
        // AUDIT GATE 9: Authority must be non-empty
        let result = AuthorityId::new("");
        assert!(result.is_err(), "Empty authority must be rejected");
        
        let result2 = AuthorityId::new("  ");
        assert!(
            result2.is_ok(),
            "Whitespace authority currently allowed (future: validate)"
        );
    }

    #[test]
    fn test_invalid_state_id_hex_format_rejected() {
        // AUDIT GATE 9: Invalid hex strings rejected
        let result = StateId::from_hex("not_hex_at_all");
        assert!(result.is_err(), "Invalid hex must be rejected");
        
        let result2 = StateId::from_hex("00");
        assert!(result2.is_err(), "Too short hex must be rejected");
        
        let result3 = StateId::from_hex(
            "00000000000000000000000000000000000000000000000000000000000000ZZZZZZZZZZ"
        );
        assert!(result3.is_err(), "Invalid hex characters must be rejected");
    }

    #[test]
    fn test_revision_with_nonexistent_parent_rejected_at_create() {
        // AUDIT GATE 9: Missing parent must be rejected explicitly
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("alice_test").unwrap();
        
        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();
        
        let fake_parent = StateId::from_hex(
            "1111111111111111111111111111111111111111111111111111111111111111"
        )
        .unwrap();
        
        let state = json!({"test": "data"});
        let result = history.create_revision(&state, Some(fake_parent));
        
        assert!(result.is_err(), "Missing parent must be rejected");
        
        match result {
            Err(StateHistoryError::MissingParent) => {
                // Correct
            }
            Err(e) => {
                panic!("Expected MissingParent error, got: {:?}", e);
            }
            Ok(_) => {
                panic!("Should reject missing parent");
            }
        }
    }

    // ======================================
    // GATE 8: GIT INDEPENDENCE VERIFICATION
    // ======================================

    #[test]
    fn test_state_history_no_git_import_verification() {
        // AUDIT GATE 8: Verify no Git module dependencies
        // This test is primarily documentation; actual verification
        // is done via code inspection (no git::* imports in state_history.rs)
        
        // Verify that StateHistory can be created without any Git infrastructure
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("git_test").unwrap();
        
        // This should succeed even if Git libs are not available
        let result = StateHistory::new(temp_dir.path(), authority);
        
        assert!(
            result.is_ok(),
            "StateHistory must work independently of Git infrastructure"
        );
    }

    #[test]
    fn test_history_ancestors_linear_chain() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("history_query_alice").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let rev_a = history.create_revision(&json!({"step": "A"}), None).unwrap();
        let rev_b = history
            .create_revision(&json!({"step": "B"}), Some(rev_a.state_id))
            .unwrap();
        let rev_c = history
            .create_revision(&json!({"step": "C"}), Some(rev_b.state_id))
            .unwrap();

        let ancestors_a = history.ancestors(rev_a.state_id).unwrap();
        assert_eq!(ancestors_a, vec![]);

        let ancestors_b = history.ancestors(rev_b.state_id).unwrap();
        assert_eq!(ancestors_b, vec![rev_a.state_id]);

        let ancestors_c = history.ancestors(rev_c.state_id).unwrap();
        assert_eq!(ancestors_c, vec![rev_b.state_id, rev_a.state_id]);
    }

    #[test]
    fn test_history_is_ancestor() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("history_query_bob").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let rev_a = history.create_revision(&json!({"step": "A"}), None).unwrap();
        let rev_b = history
            .create_revision(&json!({"step": "B"}), Some(rev_a.state_id))
            .unwrap();
        let rev_c = history
            .create_revision(&json!({"step": "C"}), Some(rev_b.state_id))
            .unwrap();

        assert!(history.is_ancestor(rev_a.state_id, rev_b.state_id));
        assert!(history.is_ancestor(rev_a.state_id, rev_c.state_id));
        assert!(history.is_ancestor(rev_b.state_id, rev_c.state_id));

        assert!(!history.is_ancestor(rev_b.state_id, rev_a.state_id));
        assert!(!history.is_ancestor(rev_a.state_id, rev_a.state_id));
    }

    #[test]
    fn test_history_common_ancestor() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("history_query_charlie").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let rev_a = history.create_revision(&json!({"root": true}), None).unwrap();
        let rev_b = history
            .create_revision(&json!({"left": 1}), Some(rev_a.state_id))
            .unwrap();
        let rev_c = history
            .create_revision(&json!({"right": 2}), Some(rev_a.state_id))
            .unwrap();

        assert_eq!(
            history.common_ancestor(rev_a.state_id, rev_a.state_id),
            Some(rev_a.state_id)
        );
        assert_eq!(
            history.common_ancestor(rev_a.state_id, rev_b.state_id),
            Some(rev_a.state_id)
        );
        assert_eq!(
            history.common_ancestor(rev_b.state_id, rev_c.state_id),
            Some(rev_a.state_id)
        );
    }

    #[test]
    fn test_history_relationship() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("history_query_diana").unwrap();

        let mut history = StateHistory::new(temp_dir.path(), authority).unwrap();

        let rev_a = history.create_revision(&json!({"step": "A"}), None).unwrap();
        let rev_b = history
            .create_revision(&json!({"step": "B"}), Some(rev_a.state_id))
            .unwrap();
        let rev_c = history
            .create_revision(&json!({"right": 2}), Some(rev_a.state_id))
            .unwrap();

        assert_eq!(
            history.relationship(rev_a.state_id, rev_a.state_id).unwrap(),
            StateRelationship::Identity
        );
        assert_eq!(
            history.relationship(rev_a.state_id, rev_b.state_id).unwrap(),
            StateRelationship::Ancestor
        );
        assert_eq!(
            history.relationship(rev_b.state_id, rev_a.state_id).unwrap(),
            StateRelationship::Descendant
        );
        assert_eq!(
            history.relationship(rev_b.state_id, rev_c.state_id).unwrap(),
            StateRelationship::Diverged
        );
    }
}

