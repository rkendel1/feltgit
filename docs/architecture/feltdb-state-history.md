# FeltDB State History - Durable Application State Model

## Executive Summary

FeltDB PR #6 establishes a durable application-state history model as a FeltDB-native abstraction, independent of Git. This document records what has been proven through executable Rust tests.

The model enables:
- **Deterministic state identity** via content-addressed hashing
- **Explicit causal ancestry** through parent references
- **Explicit authority identity** for audit and provenance
- **Durable persistence** surviving process restart
- **Immutable revisions** with no silent state mutation

## PROVEN Capabilities

### 1. Deterministic State Identity

**Claim:** Same canonical state always produces the same state identity.

**Evidence:**
- Test: `test_state_id_deterministic`
- Implementation: `CanonicalState::from_json() → calculate_state_id()`
- SHA256 hash of canonicalized JSON ensures bit-for-bit reproducibility
- Multiple invocations of `calculate_state_id()` on identical input produce identical output

**Canonical JSON Representation:**
```
Input:  {"name":"Randy","role":"admin"}
        {"role":"admin","name":"Randy"}  // Different key order

Output: {"name":"Randy","role":"admin"}  // Keys sorted alphabetically
        {"name":"Randy","role":"admin"}  // Identical

StateId: SHA256(canonical_json) = same hash
```

### 2. State Identity Independent of Key Ordering

**Claim:** JSON objects with different key orderings produce identical state identity.

**Evidence:**
- Test: `test_json_key_ordering_same_identity`
- Test: `test_canonical_json_nested_objects`
- Implementation: `CanonicalState::canonicalize_value()` recursively sorts all object keys
- Nested objects are also canonicalized (keys sorted recursively)
- Result: Any permutation of keys produces identical state_id

### 3. Different State Produces Different Identity

**Claim:** Non-identical state content always produces different state identities.

**Evidence:**
- Test: `test_state_id_different_content`
- Semantic difference: `{"name":"Randy"}` vs `{"name":"Alice"}` → different hashes
- SHA256 collision resistance ensures different content → different identity

### 4. Deterministic Calculation

**Claim:** State identity calculation is deterministic and repeatable.

**Evidence:**
- Test: `test_state_id_hex_round_trip`
- Hex serialization round-trips without loss: `StateId → hex_string → StateId` maintains equality
- No random elements, timestamps, or process-local state in the calculation
- Function signature: `calculate_state_id(CanonicalState) → StateId` is pure

### 5. Causal Parent Tracking

**Claim:** State revisions can reference their immediate predecessor.

**Evidence:**
- Test: `test_state_revision_with_parent`
- Test: `test_multi_step_history_restart`
- Implementation: `StateRevision { state_id, parent: Option<StateId>, authority }`
- Root revisions have `parent = None`
- Child revisions explicitly reference their parent's `StateId`
- Parent reference survives restart and reload

### 6. Explicit Authority Identity

**Claim:** Authority identity is explicit, stable, and persisted.

**Evidence:**
- Test: `test_authority_persisted`
- Test: `test_authority_independent_from_state_id`
- Test: `test_same_state_different_authority_distinct`
- Implementation: `AuthorityId { id: String }`
  - Must be non-empty and valid UTF-8
  - Serialized as part of revision JSON
  - Persisted to disk alongside state_id and parent
  - Does NOT affect state identity calculation

**Example:**
```
State:     {"data":"same"}
Authority: alice      → StateRevision { state_id: X, authority: alice, ... }
Authority: bob        → StateRevision { state_id: X, authority: bob, ... }

state_id is identical (content-addressed)
authority is different (explicit, independent)
```

### 7. Durable Persistence

**Claim:** Revisions persist to disk and survive process restart.

**Evidence:**
- Test: `test_persistence_write_and_load`
- Test: `test_persistence_restart_recovery`
- Test: `test_multi_step_history_restart`
- Implementation:
  - `StateHistory::create_revision()` persists via `persist_revision()`
  - Each revision written to disk as `{state_id_hex}.json`
  - JSON serialization of `StateRevision { state_id, parent, authority, state }`
  - Reload performed by `StateHistory::load_all_revisions()` on initialization

**Process:**
```
Process A:
  StateHistory.create_revision(state, None, authority_alice)
  → persists to disk: {state_id}.json

Process A terminates.

Process B (restart):
  StateHistory::new(storage_dir, authority_alice)
  → load_all_revisions() reads all .json files from storage_dir
  → deserialization from JSON
  → integrity verification via verify()
  → in-memory map rebuilt

Result: Exact equality of reloaded revision
```

### 8. Restart Recovery

**Claim:** Multi-step revision history survives process restart with parent references intact.

**Evidence:**
- Test: `test_multi_step_history_restart`
- Sequence:
  ```
  Process A:
    state1 = {"step": 1}
    rev1 = create_revision(state1, None, authority)          // parent = None
    state2 = {"step": 2}
    rev2 = create_revision(state2, rev1.state_id, authority) // parent = rev1.state_id
    
  Process A terminates.
  
  Process B:
    load rev1 → rev1.state_id unchanged
    load rev2 → rev2.parent == rev1.state_id ✓
  ```

### 9. Immutability

**Claim:** Persisted revisions cannot be silently mutated.

**Evidence:**
- Test: `test_immutability_no_silent_mutation`
- Test: `test_invalid_state_identity_rejected`
- Implementation: `StateRevision::verify()` performs integrity check
  - Recalculates state_id from persisted state
  - Rejects if calculated != persisted (invalid_state_identity)
  - State is stored as immutable string in StateRevision struct
- New state creates new revision: Different state_id → different entry in map
- Original revision remains unchanged

### 10. Validation: Missing Parent Rejection

**Claim:** Creating a revision with a nonexistent parent is rejected.

**Evidence:**
- Test: `test_missing_parent_rejected`
- Implementation:
  ```rust
  if let Some(parent_id) = parent {
      if !self.revisions.contains_key(&parent_id) {
          return Err(StateHistoryError::MissingParent);
      }
  }
  ```
- Prevents dangling causal ancestry
- Parent must be previously created and persisted

### 11. Validation: Invalid State Identity Rejection

**Claim:** A revision with mismatched state_id is rejected at verification.

**Evidence:**
- Test: `test_invalid_state_identity_rejected`
- Implementation: `StateRevision::verify()`
  ```rust
  let canonical = CanonicalState::from_json_str(&self.state)?;
  let calculated_id = calculate_state_id(&canonical);
  if calculated_id != self.state_id {
      Err(StateHistoryError::InvalidStateIdentity)
  }
  ```
- Rejects if state content doesn't match claimed state_id
- Ensures content-addressing integrity

### 12. Validation: Invalid Authority Rejection

**Claim:** AuthorityId with empty string is rejected.

**Evidence:**
- Test: `test_state_revision_invalid_authority`
- Implementation:
  ```rust
  pub fn new(id: impl Into<String>) -> Result<Self, StateHistoryError> {
      let id = id.into();
      if id.is_empty() {
          return Err(StateHistoryError::InvalidAuthority);
      }
      Ok(Self { id })
  }
  ```

### 13. Duplicate Revision Behavior

**Claim:** Creating identical revision twice is idempotent.

**Evidence:**
- Test: `test_duplicate_revision_idempotent`
- Implementation:
  ```rust
  if let Some(existing) = self.revisions.get(&revision.state_id) {
      if existing == &revision {
          return Ok(revision);  // Idempotent return
      }
      return Err(StateHistoryError::DuplicateRevision);
  }
  ```
- Deterministic: calling twice with identical inputs produces identical state_id
- Second call succeeds with same result (no error)
- Prevents accidental duplicate creation but ensures idempotency

## NOT PROVEN (Out of Scope for PR #6)

The following capabilities are explicitly **NOT** addressed in this PR:

### Replication
- No peer-to-peer message exchange
- No remote authority sync
- No network transport
- Each authority maintains its own StateHistory instance

**Why out of scope:** PR #6 establishes durable single-authority state history. Replication is a later architectural layer.

### Concurrent Authorities
- No quorum formation
- No consensus protocol
- No distributed coordination
- No voting or conflict resolution

**Why out of scope:** Authority identity is explicit but not coordinated. Each authority independently creates revisions under its own identity.

### Conflict Resolution
- No merge algorithm
- No CRDT semantics
- No automatic reconciliation
- No policy for resolving divergent histories

**Why out of scope:** PR #4 proved that deterministic reconciliation is *possible* when both parties have the same authority. PR #6 is about each authority owning its own history.

### Distributed Convergence
- No gossip protocol
- No eventual consistency model
- No causal delivery guarantees
- No happened-before relationship across authorities

**Why out of scope:** This is multi-authority territory, requiring replication and policy.

### Network Transport
- No HTTP/gRPC/custom protocol
- No wire format
- No serialization for network transport
- No keepalive or heartbeat

**Why out of scope:** Persistence is local file-based. Network is a later layer.

### Performance Scaling
- No benchmarks provided
- No distributed index
- No bloom filters
- No optimization for large histories

**Why out of scope:** First version prioritizes correctness and clarity. Scaling is future work.

### Git Interoperability
- No automatic Git commit export
- No Git ref synchronization
- No Git state blob mapping
- No Git transport

**Why out of scope:** Git was a proven substrate for the earlier experiments. FeltDB now has its own model. Future work can map between them, but they are not coupled.

## Architecture Overview

```
    Application State
           ↓
   Canonical State
    (JSON, key-sorted)
           ↓
       StateId
   (SHA256 hash)
           ↓
                    StateRevision
                   ┌────┼────┬────┐
                   ↓    ↓    ↓    ↓
              state_id parent authority state
                   ↓
            Durable Storage
           (filesystem .json)
```

### Key Types

#### StateId
- **Definition:** 32-byte SHA256 hash
- **Derivation:** Content-addressed from canonical JSON
- **Properties:** Deterministic, immutable, globally unique
- **Serialization:** 64-char hex string

#### AuthorityId
- **Definition:** Non-empty UTF-8 string
- **Semantics:** "This revision was authored under authority X"
- **Properties:** Stable, explicit, independent of state identity
- **Example:** `"alice"`, `"bob"`, `"service-node-1"`

#### StateRevision
```rust
pub struct StateRevision {
    pub state_id: StateId,           // Content-addressed identity
    pub parent: Option<StateId>,     // Immediate predecessor (if any)
    pub authority: AuthorityId,      // Author identity
    pub state: String,               // Canonical JSON state
}
```

#### StateHistory
```rust
pub struct StateHistory {
    storage_dir: PathBuf,            // Directory for .json files
    revisions: BTreeMap<StateId, StateRevision>,  // In-memory cache
    authority: AuthorityId,          // This history's author
}
```

### Public API

#### Creating a Revision
```rust
let state = serde_json::json!({"name": "Randy", "role": "admin"});
let authority = AuthorityId::new("alice")?;
let mut history = StateHistory::new("./storage", authority)?;

let revision = history.create_revision(&state, None)?;
// revision.state_id is now deterministically derived
// revision.parent is None (root revision)
// revision.authority is "alice"
// revision.state is persisted to disk
```

#### Chaining Revisions
```rust
let state2 = serde_json::json!({"name": "Randy", "role": "admin", "verified": true});
let revision2 = history.create_revision(&state2, Some(revision.state_id))?;
// revision2.parent == revision.state_id
```

#### Loading After Restart
```rust
let history = StateHistory::new("./storage", authority)?;
let loaded = history.load_revision(state_id)?;
// Exact equality with previously persisted revision
```

## Validation Guarantees

| Validation | Behavior | Evidence |
|-----------|----------|----------|
| Missing parent | Rejected with `MissingParent` error | test_missing_parent_rejected |
| Invalid state_id | Rejected by `verify()` with `InvalidStateIdentity` | test_invalid_state_identity_rejected |
| Empty authority | Rejected with `InvalidAuthority` error | test_state_revision_invalid_authority |
| Duplicate identical revision | Idempotent success | test_duplicate_revision_idempotent |
| Different content, same authority | Different state_id, new revision | test_immutability_no_silent_mutation |

## Test Coverage

Total: **21 comprehensive tests**, all passing ✓

### Identity Tests (4)
- `test_state_id_deterministic` - Same input → same output
- `test_state_id_different_content` - Different input → different output
- `test_json_key_ordering_same_identity` - Key reordering → same output
- `test_state_id_hex_round_trip` - Hex serialization round-trip

### History Tests (4)
- `test_state_revision_creation` - Root revision has no parent
- `test_state_revision_with_parent` - Child revision references parent
- `test_persistence_restart_recovery` - Survives process restart
- `test_multi_step_history_restart` - Multi-step chain survives restart

### Authority Tests (4)
- `test_authority_persisted` - Authority is persisted
- `test_same_state_different_authority_distinct` - Same state, different authority → different revision metadata
- `test_authority_independent_from_state_id` - Authority doesn't affect state_id
- `test_state_revision_verification` - Verification succeeds for valid revision

### Validation Tests (4)
- `test_missing_parent_rejected` - Nonexistent parent rejected
- `test_invalid_state_identity_rejected` - Corrupted state_id rejected
- `test_state_revision_invalid_authority` - Empty authority rejected
- `test_duplicate_revision_idempotent` - Duplicate call succeeds idempotently

### Immutability Tests (2)
- `test_immutability_no_silent_mutation` - Different state → different revision
- `test_canonical_json_nested_objects` - Nested structures don't mutate

### Isolation Tests (1)
- `test_no_reconciliation_occurs` - Creating revisions doesn't reconcile histories

### Utility Tests (2)
- `test_persistence_write_and_load` - Basic persistence round-trip
- `test_all_revisions_order` - All revisions retrievable

## Compilation and Testing

### Build
```bash
cd /home/runner/work/feltgit/feltgit
cargo build --lib --no-default-features --features state-history
```

### Run Tests
```bash
cargo test --lib state_history --no-default-features --features state-history
```

### Results
```
running 21 tests
test state_history::tests::test_authority_independent_from_state_id ... ok
test state_history::tests::test_canonical_json_nested_objects ... ok
test state_history::tests::test_authority_persisted ... ok
test state_history::tests::test_all_revisions_order ... ok
test state_history::tests::test_duplicate_revision_idempotent ... ok
test state_history::tests::test_immutability_no_silent_mutation ... ok
test state_history::tests::test_invalid_state_identity_rejected ... ok
test state_history::tests::test_json_key_ordering_same_identity ... ok
test state_history::tests::test_missing_parent_rejected ... ok
test state_history::tests::test_persistence_restart_recovery ... ok
test state_history::tests::test_no_reconciliation_occurs ... ok
test state_history::tests::test_persistence_write_and_load ... ok
test state_history::tests::test_state_id_deterministic ... ok
test state_history::tests::test_state_id_different_content ... ok
test state_history::tests::test_state_id_hex_round_trip ... ok
test state_history::tests::test_state_revision_creation ... ok
test state_history::tests::test_state_revision_invalid_authority ... ok
test state_history::tests::test_multi_step_history_restart ... ok
test state_history::tests::test_same_state_different_authority_distinct ... ok
test state_history::tests::test_state_revision_verification ... ok
test state_history::tests::test_state_revision_with_parent ... ok

test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured
```

## Design Decisions

### 1. JSON as State Format
- **Why:** Application state is commonly modeled as hierarchical data
- **Limitation:** Arrays are out of scope; only objects and primitives
- **Benefit:** Canonical representation via key sorting is well-defined

### 2. SHA256 for State Identity
- **Why:** Cryptographically secure, collision-resistant, widely used
- **Benefit:** Prevents accidental or adversarial collisions
- **Trade-off:** Fixed 32-byte output; no variable-length hashes

### 3. File-Based Persistence
- **Why:** Simple, reliable, filesystem-native
- **Limitation:** No distributed consensus; single authority per directory
- **Benefit:** Survives process restart; no network dependency

### 4. Immutable Revisions
- **Why:** Prevents silent state mutation; enables auditing
- **Mechanism:** StateRevision is persisted, not updated; new state creates new revision
- **Benefit:** Clear semantics for causality and authority

### 5. Explicit Authority Identity
- **Why:** Enables audit trail; doesn't assume single author
- **Limitation:** No authority election or quorum formation
- **Benefit:** Separates "who created this" from "is this state correct"

## Security Considerations

### Content-Addressed Identity
- **Benefit:** Impossible to create two distinct revisions with identical content
- **Relies on:** SHA256 collision resistance
- **Assumption:** Canonical JSON doesn't have hidden differences

### Immutability Verification
- **Mechanism:** `StateRevision::verify()` recalculates state_id
- **Enforcement:** Loaded revisions must pass verification
- **Assumption:** Persisted JSON is trustworthy (no tampering in transit)

### Authority Integrity
- **Scope:** AuthorityId is explicit but not cryptographically signed
- **Limitation:** Doesn't prevent authority spoofing
- **Note:** Signing is a future layer (e.g., per-revision signatures)

### No Network Exposure
- **Benefit:** No network-based vulnerabilities in this PR
- **Assumption:** Filesystem is trustworthy

## Future Directions

### Phase 2: Replication
- Gossip protocol for peer discovery
- State synchronization across authorities
- Causal delivery guarantees

### Phase 3: Conflict Resolution
- Multi-authority merge algorithm
- Policy-based conflict handling
- CRDT or other convergence semantics

### Phase 4: Signing and Verification
- Per-revision digital signatures
- Authority key rotation
- Signed audit trail

### Phase 5: Query and Index
- Time-range queries
- Authority-filtered history
- Merkle tree indexing for large histories

## Conclusion

PR #6 establishes that FeltDB can own application-state identity and durable causal history independently of Git. The model is:
- **Deterministic:** Same input always produces same state_id
- **Durable:** Survives process restart and reload
- **Immutable:** Revisions cannot be silently mutated
- **Auditable:** Explicit authority identity for each revision
- **Validated:** Rejects invalid ancestry and corrupted state

This foundation is sufficient for FeltDB to serve as an independent state versioning system, with Git as an optional substrate for the earlier experiments—not as the core model.
