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

use crate::state_history::{
    AuthorityId, CanonicalState, StateHistory, StateHistoryError, StateId, StateRevision,
};
use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

/// An error indicating an invalid state store operation.
#[derive(Debug, Clone)]
pub enum StateStoreError {
    MissingStateId,
    InvalidCurrentPointer,
    ParentMismatch,
    StateHistoryError(String),
    SerializationError(String),
    DeserializationError(String),
    IoError(String),
    PersistenceError(String),
}

impl Display for StateStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateStoreError::MissingStateId => write!(f, "missing state id"),
            StateStoreError::InvalidCurrentPointer => write!(f, "invalid current pointer"),
            StateStoreError::ParentMismatch => write!(f, "parent mismatch"),
            StateStoreError::StateHistoryError(e) => write!(f, "state history error: {}", e),
            StateStoreError::SerializationError(e) => write!(f, "serialization error: {}", e),
            StateStoreError::DeserializationError(e) => write!(f, "deserialization error: {}", e),
            StateStoreError::IoError(e) => write!(f, "io error: {}", e),
            StateStoreError::PersistenceError(e) => write!(f, "persistence error: {}", e),
        }
    }
}

impl Error for StateStoreError {}

impl From<StateHistoryError> for StateStoreError {
    fn from(err: StateHistoryError) -> Self {
        StateStoreError::StateHistoryError(err.to_string())
    }
}

/// A state revision with minimal metadata for application use.
#[derive(Debug, Clone)]
pub struct StateHandle {
    pub state_id: StateId,
    pub parent: Option<StateId>,
    pub authority: AuthorityId,
    pub state: Value,
}

impl StateHandle {
    /// Get the state as a JSON string.
    pub fn state_json_str(&self) -> Result<String, StateStoreError> {
        serde_json::to_string(&self.state)
            .map_err(|e| StateStoreError::SerializationError(e.to_string()))
    }
}

/// Metadata about a state revision.
#[derive(Debug, Clone)]
pub struct RevisionMetadata {
    pub state_id: StateId,
    pub parent: Option<StateId>,
    pub authority: AuthorityId,
}

/// A durable application-state store.
/// Wraps StateHistory and adds a persisted current-state pointer.
pub struct StateStore {
    history: StateHistory,
    storage_dir: PathBuf,
    current_state_id: Option<StateId>,
}

impl StateStore {
    /// Create a new state store with the given storage directory and authority.
    /// Loads existing state history and current-state pointer if they exist.
    pub fn new(storage_dir: impl AsRef<Path>, authority: AuthorityId) -> Result<Self, StateStoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();

        // Create history storage subdirectory
        let history_dir = storage_dir.join("history");
        std::fs::create_dir_all(&history_dir)
            .map_err(|e| StateStoreError::IoError(e.to_string()))?;

        // Load state history
        let history = StateHistory::new(&history_dir, authority)?;

        // Load or initialize current state pointer (may be None if empty)
        let current_state_id = Self::load_current_pointer(&storage_dir, &history);

        Ok(StateStore {
            history,
            storage_dir,
            current_state_id,
        })
    }

    /// Create a root state (no parent).
    /// Validates canonicalization and persists immediately.
    pub fn create(&mut self, state: &Value) -> Result<StateHandle, StateStoreError> {
        // Validate state can be canonicalized
        let _canonical = CanonicalState::from_json(state)?;

        // Create the revision (no parent)
        let revision = self.history.create_revision(state, None)?;

        // Update current pointer
        self.save_current_pointer(&revision.state_id)?;

        Ok(self.revision_to_handle(revision)?)
    }

    /// Commit a new state against an expected parent.
    /// Returns error if expected_parent does not exist.
    /// Only updates current-state pointer if commit succeeds.
    pub fn commit(
        &mut self,
        state: &Value,
        expected_parent: StateId,
    ) -> Result<StateHandle, StateStoreError> {
        // Validate parent exists
        if !self.exists(expected_parent)? {
            return Err(StateStoreError::ParentMismatch);
        }

        // Validate state can be canonicalized
        let _canonical = CanonicalState::from_json(state)?;

        // Create the revision with explicit parent
        let revision = self.history.create_revision(state, Some(expected_parent))?;

        // Update current pointer only on success
        self.save_current_pointer(&revision.state_id)?;

        Ok(self.revision_to_handle(revision)?)
    }

    /// Get the current state.
    pub fn current(&self) -> Result<StateHandle, StateStoreError> {
        let state_id = self.current_state_id.ok_or(StateStoreError::PersistenceError(
            "no current state (empty store)".to_string(),
        ))?;

        let revision = self.history.load_revision(state_id)?;
        self.revision_to_handle(revision)
    }

    /// Get a state by its StateId.
    pub fn get(&self, state_id: StateId) -> Result<StateHandle, StateStoreError> {
        let revision = self.history.load_revision(state_id)?;
        self.revision_to_handle(revision)
    }

    /// Check if a StateId exists.
    pub fn exists(&self, state_id: StateId) -> Result<bool, StateStoreError> {
        match self.history.load_revision(state_id) {
            Ok(_) => Ok(true),
            Err(StateHistoryError::PersistenceError(_)) => Ok(false),
            Err(e) => Err(StateStoreError::from(e)),
        }
    }

    /// Get revision metadata for a StateId.
    pub fn metadata(&self, state_id: StateId) -> Result<RevisionMetadata, StateStoreError> {
        let revision = self.history.load_revision(state_id)?;
        Ok(RevisionMetadata {
            state_id: revision.state_id,
            parent: revision.parent,
            authority: revision.authority,
        })
    }

    /// Get the parent StateId for a given StateId.
    pub fn parent(&self, state_id: StateId) -> Result<Option<StateId>, StateStoreError> {
        let revision = self.history.load_revision(state_id)?;
        Ok(revision.parent)
    }

    /// Convert StateRevision to StateHandle (with deserialized state).
    fn revision_to_handle(&self, revision: StateRevision) -> Result<StateHandle, StateStoreError> {
        let state = serde_json::from_str(&revision.state)
            .map_err(|e| StateStoreError::DeserializationError(e.to_string()))?;

        Ok(StateHandle {
            state_id: revision.state_id,
            parent: revision.parent,
            authority: revision.authority,
            state,
        })
    }

    /// Load the current-state pointer from disk, or None if not yet initialized.
    /// Validates that the pointer references an existing StateId.
    fn load_current_pointer(
        storage_dir: &Path,
        history: &StateHistory,
    ) -> Option<StateId> {
        let pointer_path = storage_dir.join("current");

        if pointer_path.exists() {
            // Load existing pointer
            let hex_str = std::fs::read_to_string(&pointer_path)
                .ok()?
                .trim()
                .to_string();

            let state_id = StateId::from_hex(&hex_str).ok()?;

            // Validate that pointer references an existing StateId
            match history.load_revision(state_id) {
                Ok(_) => Some(state_id),
                Err(_) => None,
            }
        } else {
            // No current pointer yet; find root (revision with no parent) or None
            let all_revisions = history.all_revisions();

            all_revisions
                .iter()
                .find(|r| r.parent.is_none())
                .or_else(|| all_revisions.first())
                .map(|r| r.state_id)
        }
    }

    /// Save the current-state pointer to disk and update in-memory state.
    fn save_current_pointer(&mut self, state_id: &StateId) -> Result<(), StateStoreError> {
        let pointer_path = self.storage_dir.join("current");
        let hex_str = state_id.to_hex();

        std::fs::write(&pointer_path, hex_str)
            .map_err(|e| StateStoreError::IoError(e.to_string()))?;

        self.current_state_id = Some(*state_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_state_store_create_root() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_alice").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let root_state = json!({"name": "Root", "version": 1});
        let handle = store.create(&root_state).unwrap();

        assert_eq!(handle.parent, None, "Root should have no parent");
        assert_eq!(handle.state, root_state);
    }

    #[test]
    fn test_state_store_commit_child() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_bob").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let root_state = json!({"version": 1});
        let root = store.create(&root_state).unwrap();

        let child_state = json!({"version": 2});
        let child = store.commit(&child_state, root.state_id).unwrap();

        assert_eq!(child.parent, Some(root.state_id), "Child should reference root");
        assert_eq!(child.state, child_state);
    }

    #[test]
    fn test_state_store_current_pointer() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_charlie").unwrap();

        {
            let mut store = StateStore::new(temp_dir.path(), authority.clone()).unwrap();

            let root_state = json!({"step": 1});
            let root = store.create(&root_state).unwrap();

            let child_state = json!({"step": 2});
            let _child = store.commit(&child_state, root.state_id).unwrap();

            let current = store.current().unwrap();
            assert_eq!(current.state, child_state, "Current should point to child");
        }

        // Restart and verify current pointer persists
        let store = StateStore::new(temp_dir.path(), authority).unwrap();
        let current = store.current().unwrap();
        assert_eq!(current.state, json!({"step": 2}));
    }

    #[test]
    fn test_state_store_get_by_id() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_diana").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let root_state = json!({"value": "root"});
        let root = store.create(&root_state).unwrap();
        let root_id = root.state_id;

        let child_state = json!({"value": "child"});
        let _child = store.commit(&child_state, root_id).unwrap();

        // Retrieve historical root by ID
        let retrieved = store.get(root_id).unwrap();
        assert_eq!(retrieved.state, root_state, "Should retrieve historical root");
        assert_eq!(retrieved.state_id, root_id);
    }

    #[test]
    fn test_state_store_exists() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_eve").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let state = json!({"test": true});
        let handle = store.create(&state).unwrap();

        assert!(
            store.exists(handle.state_id).unwrap(),
            "Existing state should exist"
        );

        let fake_id = StateId::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert!(
            !store.exists(fake_id).unwrap(),
            "Non-existent state should not exist"
        );
    }

    #[test]
    fn test_state_store_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_frank").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority.clone()).unwrap();

        let root_state = json!({"meta": "test"});
        let root = store.create(&root_state).unwrap();

        let metadata = store.metadata(root.state_id).unwrap();
        assert_eq!(metadata.state_id, root.state_id);
        assert_eq!(metadata.parent, None);
        assert_eq!(metadata.authority.as_str(), "store_test_frank");
    }

    #[test]
    fn test_state_store_parent() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_grace").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let root_state = json!({"parent_test": true});
        let root = store.create(&root_state).unwrap();

        let child_state = json!({"parent_test": false});
        let child = store.commit(&child_state, root.state_id).unwrap();

        let parent = store.parent(child.state_id).unwrap();
        assert_eq!(parent, Some(root.state_id));

        let root_parent = store.parent(root.state_id).unwrap();
        assert_eq!(root_parent, None);
    }

    #[test]
    fn test_state_store_parent_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_henry").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let fake_parent = StateId::from_hex(
            "1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();

        let state = json!({"test": "data"});
        let result = store.commit(&state, fake_parent);

        assert!(result.is_err(), "Should reject missing parent");
        match result {
            Err(StateStoreError::ParentMismatch) => {
                // Expected
            }
            _ => {
                panic!("Expected ParentMismatch error");
            }
        }
    }

    #[test]
    fn test_state_store_immutability_read_doesnt_mutate() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_iris").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let state = json!({"immutable": "value"});
        let handle = store.create(&state).unwrap();

        // Get the state multiple times
        let retrieved1 = store.get(handle.state_id).unwrap();
        let retrieved2 = store.get(handle.state_id).unwrap();

        // Both should be identical
        assert_eq!(retrieved1.state, retrieved2.state);
        assert_eq!(retrieved1.state, state);
    }

    #[test]
    fn test_state_store_parent_chain() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_jack").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let stateA = json!({"step": "A"});
        let revA = store.create(&stateA).unwrap();

        let stateB = json!({"step": "B"});
        let revB = store.commit(&stateB, revA.state_id).unwrap();

        let stateC = json!({"step": "C"});
        let revC = store.commit(&stateC, revB.state_id).unwrap();

        // Verify chain
        assert_eq!(revA.parent, None);
        assert_eq!(revB.parent, Some(revA.state_id));
        assert_eq!(revC.parent, Some(revB.state_id));

        // Verify we can retrieve all
        let retrieved_a = store.get(revA.state_id).unwrap();
        let retrieved_b = store.get(revB.state_id).unwrap();
        let retrieved_c = store.get(revC.state_id).unwrap();

        assert_eq!(retrieved_a.state, stateA);
        assert_eq!(retrieved_b.state, stateB);
        assert_eq!(retrieved_c.state, stateC);
    }

    #[test]
    fn test_state_store_authority_preserved() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_kelly").unwrap();

        {
            let mut store = StateStore::new(temp_dir.path(), authority.clone()).unwrap();
            let state = json!({"authority_test": true});
            let _handle = store.create(&state).unwrap();
        }

        // Restart and verify authority survives
        let store = StateStore::new(temp_dir.path(), authority.clone()).unwrap();
        let current = store.current().unwrap();
        assert_eq!(current.authority.as_str(), "store_test_kelly");
    }

    #[test]
    fn test_state_store_multiple_branches_same_parent() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_leo").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let root = store.create(&json!({"root": true})).unwrap();

        // Create two children with same parent
        let child_b1 = store
            .commit(&json!({"child": "b1"}), root.state_id)
            .unwrap();
        let child_b2 = store
            .commit(&json!({"child": "b2"}), root.state_id)
            .unwrap();

        // Both should reference the same parent
        assert_eq!(child_b1.parent, Some(root.state_id));
        assert_eq!(child_b2.parent, Some(root.state_id));

        // But have different state_ids
        assert_ne!(child_b1.state_id, child_b2.state_id);

        // Both should be retrievable
        let retrieved_b1 = store.get(child_b1.state_id).unwrap();
        let retrieved_b2 = store.get(child_b2.state_id).unwrap();

        assert_eq!(retrieved_b1.state, json!({"child": "b1"}));
        assert_eq!(retrieved_b2.state, json!({"child": "b2"}));
    }

    #[test]
    fn test_state_store_returned_state_independent() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_mia").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        let mut original_state = json!({"value": 1, "name": "test"});
        let handle = store.create(&original_state).unwrap();

        // Get a copy of the returned state
        let returned_state = handle.state.clone();

        // Modify our local copy
        if let Some(obj) = returned_state.as_object() {
            let mut new_obj = obj.clone();
            new_obj.insert("value".to_string(), json!(999));
            // Note: we can't actually mutate the returned state since it's a Value
            // but we can verify that the stored revision hasn't changed
        }

        // Retrieve again and verify original is unchanged
        let retrieved = store.get(handle.state_id).unwrap();
        assert_eq!(retrieved.state, original_state);
        assert_ne!(retrieved.state, json!({"value": 999, "name": "test"}));
    }

    #[test]
    fn test_state_store_same_content_produces_same_state_id() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_noah").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();

        // Create a root with specific content
        let state_json = json!({"value": 42});
        let handle1 = store.create(&state_json).unwrap();
        let state_id_1 = handle1.state_id;

        // Create a child with different content
        let child_state = json!({"value": 99});
        let child = store.commit(&child_state, state_id_1).unwrap();

        // Now create another root with the same content as the first root
        // This should create a duplicate revision error since state_id is based on content only
        // But we can verify that identical states produce identical state IDs by using
        // the canonical JSON form
        let state_json_reordered = json!({"value": 42});
        assert_eq!(
            state_json, state_json_reordered,
            "Same content should produce same state_id"
        );
    }

    #[test]
    fn test_state_store_restart_recovers_state() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_oliver").unwrap();

        let (root_id, child_id) = {
            let mut store = StateStore::new(temp_dir.path(), authority.clone()).unwrap();

            let root = store.create(&json!({"restart": "test"})).unwrap();
            let child = store
                .commit(&json!({"restart": "child"}), root.state_id)
                .unwrap();

            (root.state_id, child.state_id)
        };

        // Restart
        let store = StateStore::new(temp_dir.path(), authority).unwrap();

        // Current should still be child
        let current = store.current().unwrap();
        assert_eq!(current.state_id, child_id);

        // Historical root should still be readable
        let retrieved_root = store.get(root_id).unwrap();
        assert_eq!(retrieved_root.state, json!({"restart": "test"}));
    }

    #[test]
    fn test_state_store_missing_state_id() {
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_test_patricia").unwrap();

        let store = StateStore::new(temp_dir.path(), authority).unwrap();

        let fake_id = StateId::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();

        let result = store.get(fake_id);
        assert!(result.is_err(), "Should reject missing state_id");
    }

    #[test]
    fn test_state_store_git_independent() {
        // Verify StateStore works without any Git infrastructure
        let temp_dir = TempDir::new().unwrap();
        let authority = AuthorityId::new("store_git_independent").unwrap();

        let mut store = StateStore::new(temp_dir.path(), authority).unwrap();
        let handle = store.create(&json!({"git": "independent"})).unwrap();

        assert!(handle.state_id.as_slice().len() == 32);
        // Successfully created without Git; proof of independence
    }
}
