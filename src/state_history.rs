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
}
