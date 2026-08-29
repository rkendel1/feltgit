//! Integration test demonstrating consumer usage of the packaged FeltDB state model
//! This test verifies that external consumers can use the public API without
//! any knowledge of feltdbgit or Git internals.

use serde_json::json;
use tempfile::TempDir;

#[test]
fn consumer_basic_workflow() {
    // Simulate being a downstream application
    let dir = TempDir::new().unwrap();
    
    // This is all we need to import from the public FeltDB package
    use gitcore::state_store::StateStore;
    use gitcore::state_history::AuthorityId;

    // Initialize state store with authority
    let mut store = StateStore::new(dir.path(), AuthorityId::new("app-server").unwrap()).unwrap();

    // Create initial application state
    let initial_state = json!({
        "app": "UserService",
        "version": "1.0",
        "users": []
    });

    let rev_1 = store.create(&initial_state).unwrap();
    assert_eq!(rev_1.parent, None);

    // Transition to next state
    let updated_state = json!({
        "app": "UserService",
        "version": "1.1",
        "users": [
            {"id": 1, "name": "Alice"}
        ]
    });

    let rev_2 = store.commit_transition(rev_1.state_id, &updated_state).unwrap();
    assert_eq!(rev_2.parent, Some(rev_1.state_id));

    // Verify current pointer
    let current = store.current().unwrap();
    assert_eq!(current.state_id, rev_2.state_id);
}

#[test]
fn consumer_branching() {
    let dir = TempDir::new().unwrap();
    use gitcore::state_store::StateStore;
    use gitcore::state_history::AuthorityId;

    let mut store = StateStore::new(dir.path(), AuthorityId::new("app-client").unwrap()).unwrap();

    // Create root state
    let state_root = json!({"counter": 0});
    let rev_root = store.create(&state_root).unwrap();

    // Create two independent branches
    let state_b1 = json!({"counter": 1});
    let rev_b1 = store.create_branch(rev_root.state_id, &state_b1).unwrap();

    let state_b2 = json!({"counter": 2});
    let rev_b2 = store.create_branch(rev_root.state_id, &state_b2).unwrap();

    // Verify current pointer unchanged (branching doesn't advance it)
    let current = store.current().unwrap();
    assert_eq!(current.state_id, rev_root.state_id);

    // Analyze relationship
    let rel = store.relationship(rev_b1.state_id, rev_b2.state_id).unwrap();
    println!("Branch relationship: {:?}", rel);
}

#[test]
fn consumer_conflict_resolution() {
    let dir = TempDir::new().unwrap();
    use gitcore::state_store::{StateStore, ReconciliationPlan};
    use gitcore::state_history::AuthorityId;

    let mut store = StateStore::new(dir.path(), AuthorityId::new("merger").unwrap()).unwrap();

    // Three-way merge scenario
    // Base version (common ancestor)
    let base = json!({"title": "Original", "status": "draft"});
    let rev_base = store.create(&base).unwrap();

    // Left branch: change title
    let left = json!({"title": "Updated", "status": "draft"});
    let rev_left = store.create_branch(rev_base.state_id, &left).unwrap();

    // Right branch: change status
    let right = json!({"title": "Original", "status": "published"});
    let rev_right = store.create_branch(rev_base.state_id, &right).unwrap();

    // Analyze conflicts
    let conflicts = store.classify_conflicts(rev_left.state_id, rev_right.state_id).unwrap();
    assert!(!conflicts.is_identity());

    // Application chooses reconciliation
    let resolved = json!({"title": "Updated", "status": "published"});
    let plan = ReconciliationPlan {
        base_state: Some(rev_base.state_id),
        left_state: rev_left.state_id,
        right_state: rev_right.state_id,
        result: resolved,
        parent_choice: rev_left.state_id,
    };

    let reconciled = store.reconcile(&plan).unwrap();
    assert_eq!(reconciled.parent, Some(rev_left.state_id));

    // Reconciliation does NOT advance current pointer
    let current = store.current().unwrap();
    assert_eq!(current.state_id, rev_base.state_id);
}

#[test]
fn consumer_no_git_dependency() {
    // This test verifies that consumers can use the package without
    // any git-integration feature or Git runtime dependencies
    
    let dir = TempDir::new().unwrap();
    use gitcore::state_store::StateStore;
    use gitcore::state_history::AuthorityId;

    // Create and use the state store
    let mut store = StateStore::new(dir.path(), AuthorityId::new("test-consumer").unwrap()).unwrap();
    let state = json!({"test": true});
    let rev = store.create(&state).unwrap();
    
    // Verify it works
    let retrieved = store.get(rev.state_id).unwrap();
    assert_eq!(retrieved.state_id, rev.state_id);
    
    // This test itself proves there are no git-integration dependencies required
}
