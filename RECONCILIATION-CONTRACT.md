# PR #13: Reconciliation Contract and Boundary Design

## Executive Summary

PR #13 establishes the contract for reconciliation—what FeltDB can guarantee about the operation without making application authority or policy decisions. This document separates mechanism (what FeltDB does) from policy (what applications decide) before any reconciliation implementation.

The architectural boundary is:

```
Conflict Classification (PR #12)
    ↓
[RECONCILIATION BOUNDARY - PR #13]
    ↓
Reconciliation Intent (Application decides)
    ↓
Reconciliation Strategy (Application supplies)
    ↓
Reconciliation Result (FeltDB provides immutable state)
    ↓
Commit Transition (PR #8 mechanism)
    ↓
Current Pointer Advance (Application controls)
```

---

## 1. Problem Statement

PR #12 established observational conflict classification: it answers "what changed and how are those changes related?"

PR #13 must establish what reconciliation *means* without:
- Assuming it's a merge
- Assuming it's conflict resolution
- Assuming FeltDB chooses a winner
- Assuming any authority/policy framework

The governing principle:

**FeltDB may provide durable state mechanics and reconciliation primitives, but it must not silently decide application authority or policy.**

---

## 2. Definitions

### Reconciliation (Proposed)

**Reconciliation** is an operation that takes:
- A base state (common context)
- Two divergent states derived from that base
- An explicit resolution strategy supplied by the caller

And produces:
- Either a new candidate state (if the strategy determines a resolution)
- Or an explicit failure indicating that resolution is impossible

**What reconciliation is NOT:**
- Automatic conflict resolution (requires application policy)
- A merge (unless the application policy is "three-way merge")
- Authority selection (which replica wins, if any)
- Assumption about the "correct" value

### Candidate Result

A **candidate result** is a new state produced by reconciliation that is NOT necessarily:
- The winner
- The "correct" state
- The state that should become current

It is simply:
- A new immutable state in the store
- Derived from explicit inputs under an explicit strategy
- Available for inspection and validation
- Ready for explicit commitment if the application approves

### Strategy

A **strategy** is an application-supplied mechanism for resolving conflicts. Examples (not implemented, illustrative):

- Three-way merge: "use conflict classification to merge compatible changes"
- First-write-wins: "use timestamp to choose older state"
- Last-write-wins: "use timestamp to choose newer state"  
- Explicit authority: "use owner/writer identity to choose"
- Custom domain logic: "use application-specific rules"
- User decision: "halt and ask human"

None of these are FeltDB's responsibility.

---

## 3. Reconciliation vs Conflict Classification

### Conflict Classification (PR #12)

**Purpose**: Observe and categorize changes

**Input**: Two states, their relationship

**Output**: Classification of changes as:
- Independent (different paths)
- Convergent (same path, same value)
- Conflicting (same path, different values)

**Key Property**: No decisions made. Only observation and categorization.

### Reconciliation (PR #13 Contract)

**Purpose**: Resolve classified conflicts under application policy

**Input**: 
- Base state
- Left state  
- Right state
- Application-supplied strategy

**Output**: 
- Candidate result state OR failure
- Provenance metadata (what inputs were used, what strategy)

**Key Property**: Requires explicit application input for *every* resolution decision.

---

## 4. Mechanism vs Policy

### FeltDB's Responsibility (Mechanism)

FeltDB MUST guarantee:

1. **Identify the base** - determine common ancestor correctly
2. **Inspect changes** - classify what changed and how  
3. **Construct candidate state** - apply strategy to produce new state
4. **Validate ancestry** - confirm base/left/right are valid ancestors
5. **Validate expected parent** - confirm commit will have correct parent
6. **Create immutable resulting state** - persist without mutation
7. **Preserve provenance** - record what inputs created the result
8. **Persist the resulting revision** - ensure durability
9. **Advance current only under explicit transition semantics** - don't auto-advance

### Application's Responsibility (Policy)

Application MUST supply:

1. **Which conflicting value wins** - if any, and only if strategy requires it
2. **Whether deletion beats modification** - domain-specific choice
3. **Whether one authority outranks another** - if authorities matter
4. **Whether both values should be preserved** - multi-value resolution
5. **Whether a human must decide** - user involvement policy
6. **Domain-specific conflict rules** - application logic

---

## 5. Three-Way Semantics

### Base, Left, Right Definitions

**Base** = the common causal context from which competing states are understood

Not "the older state" or "the less authoritative state"

Rather: the revision both sides' changes are relative to

### Three-Way Reconciliation Preconditions

For three-way reconciliation to apply:

1. A common ancestor (base) must exist
2. Left and Right must both be descendants of (or equal to) base
3. The relationship must be one of:
   - **Diverged**: Both descended from base, evolved independently
   - **Identity**: Left == Right (no reconciliation needed)
   - **Ancestor/Descendant**: One is ancestor of other (potentially fast-forward, not reconciliation)

### Three-Way Reconciliation Process

Given base, left, right:

1. Compute diff(base → left) = left's changes
2. Compute diff(base → right) = right's changes
3. For each changed path:
   - If only left changed: include left's value (independent)
   - If only right changed: include right's value (independent)
   - If both changed to same value: use that value (convergent)
   - If both changed differently: invoke strategy
4. Strategy determines the outcome for each conflict

---

## 6. Relationship-Specific Behavior

### Identity (left == right)

**Relationship**: States are identical

**Current Classification**: `ConflictClassification` reports zero conflicts

**Reconciliation Behavior**:
- **NO reconciliation necessary**
- Candidate result is identical to left and right
- Potentially no-op (depending on application semantics)

**PROVEN**: `StateStore::relationship()` identifies identity correctly

---

### Ancestor/Descendant (one is ancestor of other)

**Relationship**: States form a linear history

**Current Classification**: `ConflictClassification` reports zero conflicts (no divergence)

**Reconciliation Behavior**: 
- **NOT a reconciliation case** - this is potentially a fast-forward
- If left is ancestor of right: right already incorporates left's state (and more)
- If right is ancestor of left: left already incorporates right's state (and more)
- No competing history to reconcile
- Application should not call reconciliation for linear history

**Design Decision**: Reconciliation API should allow rejection of non-divergent relationships OR require application to handle explicitly.

**Recommended**: Require application to check relationship first, only call reconciliation for Diverged.

**PROVEN**: `StateStore::relationship()` distinguishes Ancestor/Descendant from Diverged

---

### Diverged (both descended from common ancestor)

**Relationship**: States evolved independently from common ancestor

**Current Classification**: `ConflictClassification` identifies:
- Independent changes (different paths)
- Convergent changes (same path, same value)
- True conflicts (same path, different values)

**Reconciliation Behavior**:
- **Primary reconciliation case**
- Application supplies strategy
- For each conflict, strategy determines outcome
- For convergent and independent changes, auto-include in result
- Produces candidate result or explicit failure

**PROVEN**: 
- `StateStore::common_ancestor()` finds common ancestor
- `StateStore::classify_conflicts()` provides three-way classification
- Result structure contains base_state, left_changes, right_changes, path_conflicts

---

### Unrelated (no common ancestor)

**Relationship**: States have no causal relationship

**Current Classification**: `ConflictClassification` reports:
- relationship = Unrelated
- Both states treated as independent creations from empty base

**Reconciliation Behavior**: 
- Reconciliation is **possible but unusual**
- Requires explicit caller policy for "no shared base"
- Strategy must address: "what does it mean to merge states with no causal relationship?"
- Examples: merging databases from different systems, federation scenarios

**Design Decision**: Support with explicit caller awareness OR reject with error.

**Recommended**: Support with explicit base specification:
- Caller provides base = None (or empty state) explicitly
- Caller supplies strategy that handles no-common-context
- Results are still valid immutable states with provenance

**Potential Future Refinement**: Different strategies may require different base semantics:
- Merge strategy: treats no base as divergence from empty
- Overwrite strategy: treats one side as complete replacement
- Combination strategy: preserves both values (in structure)

**PROVEN**: `ConflictClassification` correctly identifies Unrelated states

---

## 7. Authority Neutrality

This principle is **non-negotiable**.

### Forbidden Silent Choices

FeltDB MUST NEVER silently choose:

- **Owner** - no assumption about which writer should win
- **Writer** - no ranking of authors or identities
- **Node** - no preference for local vs remote state
- **Timestamp** - order of changes does not imply authority
- **Latest state** - most recent is not automatically correct
- **Majority** - no voting or consensus logic
- **Local state** - no bias toward current node
- **Remote state** - no bias toward other replicas

### Explicit Authority if Needed

If an application strategy WANTS to use any of these:
- It must be **explicitly supplied** as policy
- It must be **applied by application code**, not FeltDB
- It must be **recorded in provenance** (what strategy decided)

### Provenance Requirement

Every reconciliation result must preserve:
- **base_id**: The common ancestor
- **left_id**: The left input state
- **right_id**: The right input state
- **strategy_name** (or strategy_id): What resolved conflicts
- **resolution_method**: How conflicts were decided
  - "auto_convergent" = changes independently agreed
  - "auto_independent" = changes on different paths
  - "strategy_applied" = explicit strategy chose values
  - Other application-specific methods

This allows inspection: "how did this state come to be?"

**NOT PROVEN**: Provenance fields not yet specified

---

## 8. Immutability Preservation

**Critical Invariant**: Reconciliation must never mutate either input state.

### Visual Contract

```
Base ─────┐
          │
Left ─────┼──→ reconciliation ──→ New State
          │    (immutable)
Right ────┘
```

- Base remains immutable
- Left remains immutable
- Right remains immutable
- Only result is new

### Implementation Requirements

1. Reconciliation accepts references to base/left/right states, never pointers that could be modified
2. Reconciliation returns a NEW state value
3. Reconciliation NEVER updates base/left/right in place
4. Reconciliation result is a new StateId in the store

**PROVEN**: 
- `StateStore::classify_conflicts()` accepts only immutable references
- Never calls mutable methods on input states
- Returns new classification data, never modifies inputs

---

## 9. Proposed Future API Surface

### Model C: Plan/Result Based (RECOMMENDED)

This model separates concerns cleanly:

```
Step 1: Classify
classify(left, right) 
  → ConflictClassification {
      relationship,
      base_state,
      path_conflicts,
      convergent_changes,
      true_conflicts
    }

Step 2: Apply Strategy (Application)
application_strategy(classification)
  → ResolutionPlan {
      path → chosen_value  (for conflicts)
      OR: "unable to resolve"
    }

Step 3: Construct Candidate
reconcile_with_plan(base, left, right, plan)
  → Value (new candidate state)
  OR → ReconciliationError

Step 4: Commit Explicitly
commit_transition(expected_parent, candidate_state)
  → StateId
```

**Why this model?**

1. **Clean separation**: Classification ≠ Resolution ≠ Commitment
2. **Observable intermediates**: Plan can be inspected before commit
3. **Authority control**: Application makes ALL policy decisions
4. **No hidden semantics**: Each step is explicit
5. **Testable**: Each stage can be tested independently
6. **Auditable**: Full history of decision-making visible
7. **Reversible**: Application can reject plan and try different strategy

### Alternative Models (Not Recommended)

#### Model A: Caller-Supplied Result
```
reconcile(base, left, right, application_provided_merged_state)
```

**Problems**:
- Application must understand state structure enough to build result
- Easier to accidentally create invalid states
- FeltDB's only role is validation, not construction
- Doesn't leverage FeltDB's change tracking

#### Model B: Strategy-Driven
```
reconcile(base, left, right, strategy: "merge" | "left-wins" | "right-wins" | ...)
```

**Problems**:
- Strategy names imply policies FeltDB shouldn't make
- Limits extensibility
- Hardcodes authority assumptions
- "merge" assumes merge semantics
- "left-wins" assumes identity-based authority
- Any strategy name carries implicit policy

### Recommended: Model C

```rust
pub struct ResolutionPlan {
    pub path_resolutions: BTreeMap<StatePath, Value>,
    pub auto_convergent_paths: Vec<StatePath>,
    pub auto_independent_paths: Vec<StatePath>,
}

pub fn reconcile_with_plan(
    base: StateId,
    left: StateId, 
    right: StateId,
    plan: ResolutionPlan,
) -> Result<Value, ReconciliationError>;
```

---

## 10. Atomicity and Commit Boundary

### Clean Boundary Exists

PR #8 establishes that `commit_transition()` is the atomic state mutation primitive:

```rust
pub fn commit_transition(
    &mut self,
    expected_parent: StateId,
    next_state: &Value,
) -> Result<StateHandle, StateStoreError>
```

This accepts:
- An expected parent (the current state we're building from)
- A new state value
- Atomically validates and commits

### Reconciliation Should NOT Mutate Database

**Design Decision**: Reconciliation produces an immutable `Value`, not a `StateId`.

```
reconcile_with_plan(...)
  → Value (in-memory candidate)
  
Application inspects Value

Application calls:
commit_transition(expected_parent, Value)
  → StateId (now durable)
```

**Why separate?**

1. Allows application to inspect/validate result before commit
2. Matches PR #8's transition semantics exactly
3. No "ghost" states created by reconciliation  
4. Application controls when/if result becomes durable
5. Preserves intent: application owns mutation policy

### Alternative: Reconciliation Creates StateId Directly

**Not Recommended**:

```
reconcile_with_plan(...) → StateId
```

This would:
- Create state in database without application approval
- Violate current-pointer semantics (result not necessarily current)
- Complicate undo/rollback scenarios
- Require separate "apply result" step anyway
- Add a second mutation pathway

**Rejected**.

---

## 11. Provenance Requirements

### Why Provenance Matters

A reconciled state must answer:
- "What states contributed to this?"
- "What was the common base?"
- "What strategy produced this result?"
- "Was this merged or explicitly constructed?"

Without provenance, reconciliation results are opaque.

### Semantic Necessity

**PROVEN Provenance Fields**:

1. **base_id** - required for three-way understanding
2. **left_id** - required to understand which side
3. **right_id** - required to understand which side

These are semantically necessary to answer "how did this state come to be?"

### Optional/Future Provenance

**NOT PROVEN** (decisions deferred):

- Strategy identifier or full strategy description
- Timestamp of reconciliation
- Application/requester identity
- Human-readable notes
- Conflict resolution method per path
- Whether automatic or explicit
- Prev state before reconciliation
- Next state after reconciliation

**Design Question**: Should reconciliation results be states themselves (carrying provenance) or detached candidates?

**Current Recommendation**: Provenance as optional metadata, not required in state itself.

---

## 12. Explicit Non-Goals

This PR explicitly does NOT implement:

- ❌ `reconcile()` function
- ❌ `merge()` function
- ❌ `resolve()` function
- ❌ `apply_diff()` function
- ❌ Winner selection logic
- ❌ Conflict strategy execution
- ❌ Authority policy
- ❌ CRDT resolution
- ❌ Synchronization
- ❌ Replication
- ❌ Git integration
- ❌ Automatic state mutation

This PR ONLY establishes:

- ✅ What reconciliation means
- ✅ What FeltDB can guarantee
- ✅ What applications must provide
- ✅ What boundaries exist
- ✅ What API surface might look like

---

## 13. Evidence Requirements

### PROVEN

These architectural decisions are backed by existing code:

#### Conflict Classification (PR #12)

**Evidence**: `src/state_store.rs` lines 642-702
- `classify_conflicts()` exists and works correctly
- Separates observation from decision
- Provides ConflictClassification with:
  - `relationship` (Identity/Ancestor/Descendant/Diverged/Unrelated)
  - `base_state` (common ancestor)
  - `left_changes` (diffs from base to left)
  - `right_changes` (diffs from base to right)
  - `path_conflicts` (conflicts at specific paths)

**Test Coverage**: 21 passing tests in `cargo test classify_`

**Conclusion**: Classification API is sufficient to build reconciliation on top.

#### Relationship Topology (PR #10)

**Evidence**: `src/state_store.rs` lines 577-587
- `is_ancestor()` determines if one state is ancestor of another
- `common_ancestor()` finds lowest common ancestor
- `relationship()` returns StateRelationship enum:
  - Identity
  - Ancestor
  - Descendant
  - Diverged
  - Unrelated

**Conclusion**: Topology primitives are sufficient for reconciliation pre-checks.

#### Transition Semantics (PR #8)

**Evidence**: `src/state_store.rs` lines 477-501
- `commit_transition()` accepts expected_parent
- Validates parent matches current state
- Atomically creates new revision
- Updates current pointer only on success

**Conclusion**: Transition primitive is appropriate commit boundary for reconciliation results.

#### Immutability

**Evidence**: 
- `classify_conflicts()` takes `&self, left, right` (immutable references)
- No mutable paths through conflict classification
- Results are new ConflictClassification, never mutated inputs

**Conclusion**: Immutability is preserved by existing API.

### NOT PROVEN

#### Provenance Semantics

**Claim**: Reconciliation results should preserve which inputs contributed

**Evidence**: Conceptually necessary, but not yet specified in code

**Status**: Design decision needed before implementation

**Recommendation**: Store base_id, left_id, right_id as provenance fields on reconciliation results

#### Strategy Application

**Claim**: Applications can supply arbitrary strategies

**Evidence**: Not yet implemented, design is this PR's deliverable

**Status**: API surface still being designed

**Recommendation**: ResolutionPlan model allows application-supplied strategies without FeltDB enforcement

#### Candidate Result Workflow

**Claim**: Results can be inspected before commitment

**Evidence**: Conceptually sound with current commit_transition() semantics

**Status**: Workflow pattern, not yet implemented

**Recommendation**: Document workflow, implement in Phase 2

---

## 14. Open Questions

### Design Decisions

1. **Should reconciliation reject non-diverged relationships or handle them?**
   - Recommended: Require caller to check relationship first
   - Alternative: Silently return identity/ancestor results

2. **Should reconciliation support unrelated states?**
   - Recommended: Yes, with explicit application awareness
   - Alternative: Reject with error

3. **Should reconciliation create intermediate StateId or return Value?**
   - Recommended: Return Value for inspection, require explicit commit
   - Alternative: Create StateId directly (not recommended)

4. **What provenance fields are semantically required vs optional?**
   - Recommended: base_id, left_id, right_id required
   - Others: deferred to future strategy implementation

5. **Should strategies be reusable objects or one-off functions?**
   - Recommended: One-off functions (ResolutionPlan) to avoid strategy registry/plugin system
   - Alternative: Strategy trait (introduces unnecessary indirection)

### Implementation Sequencing

1. Which comes first: reconcile_with_plan() or classification API expansion?
2. Should provenance be added to StateHandle or separate metadata?
3. How should applications implement custom strategies?
4. Should error handling distinguish "unresolvable conflict" from "implementation error"?

---

## 15. Recommended Architecture

### Phase 1: This PR (Contract Only)

- ✅ Define reconciliation boundaries
- ✅ Separate mechanism from policy
- ✅ Establish what FeltDB guarantees
- ✅ Document how applications provide policy
- ✅ Propose API surface
- ✅ Perform hostile audit

### Phase 2: Reconciliation Mechanics Implementation

- Implement `reconcile_with_plan()` function
- Accept ResolutionPlan with path resolutions
- Apply three-way merge logic for independent changes
- Apply strategy decisions for true conflicts
- Return candidate Value (not StateId)
- Ensure immutability throughout

### Phase 3: Application Integration Patterns

- Document strategy implementation
- Provide examples: merge, custom logic, human decision
- Guide on when to use which strategy
- Show workflow: classify → plan → reconcile → commit

### Phase 4: Performance and Optimization

- Profile reconciliation on large states
- Optimize diff/merge algorithms if needed
- Consider partial reconciliation for large objects

---

## 16. Hostile Audit

This section answers 10 critical questions to ensure no hidden policy sneaks into reconciliation.

### Q1: Does the proposed model assume a winner?

**Answer**: No. Model C explicitly requires strategy to decide outcomes.

**Evidence**:
- ConflictClassification marks conflicts as "Conflict", doesn't choose
- ResolutionPlan accepts application's explicit choices
- FeltDB applies plan but doesn't generate it

**Verdict**: ✅ PASSES - No hidden winner assumption

### Q2: Does "reconciliation" secretly mean "merge"?

**Answer**: No. Reconciliation is a general framework; merge is one possible strategy.

**Evidence**:
- Relationship check (Diverged vs others) is separate
- Strategy application is application's responsibility
- Alternative strategies (overwrite, combination, explicit) are possible

**Concern Addressed**: Three-way merge is one strategy, not the only one

**Verdict**: ✅ PASSES - Merge is optional strategy, not assumption

### Q3: Does authority metadata influence outcome?

**Answer**: No. Authority information (if present) only exists in provenance, not decision logic.

**Evidence**:
- ConflictClassification uses only value comparison
- ResolutionPlan uses only application-supplied strategy
- No timestamps, author info, or authority checks in core logic

**Verdict**: ✅ PASSES - Authority is metadata, not policy

### Q4: Does timestamp/order influence outcome?

**Answer**: No. Order of changes is not considered in reconciliation.

**Evidence**:
- Conflict classification uses value comparison, not temporal order
- Strategy is explicitly supplied, not derived from timestamps
- No "last-write-wins" or "first-write-wins" is automatic

**Concern**: Could strategy implementation use timestamps?
- **Yes**, but that's application's choice
- FeltDB doesn't provide timestamps
- If application wants timestamp-based, it must supply externally

**Verdict**: ✅ PASSES - No implicit temporal ordering

### Q5: Does the design require Git semantics?

**Answer**: No. Reconciliation is state-semantic, independent from Git.

**Evidence**:
- No dependencies on commits, branches, rebases
- No Git merge driver involvement
- StateStore works standalone
- ConflictClassification is pure semantic diff

**Constraint**: Application can use reconciliation independently from Git

**Verdict**: ✅ PASSES - Git independence preserved

### Q6: Does it duplicate topology logic?

**Answer**: No. Reconciliation uses existing topology primitives.

**Evidence**:
- Uses `relationship()` for Diverged check
- Uses `common_ancestor()` for base finding
- Uses `classify_conflicts()` for change categorization
- No new ancestry model introduced

**Verdict**: ✅ PASSES - No duplication, reuses PR #10 logic

### Q7: Does it duplicate diff logic?

**Answer**: No. Reconciliation uses existing diff primitives.

**Evidence**:
- Uses `diff()` for left/right changes
- Uses `classify_conflicts()` for conflict categorization
- No new diff algorithm
- Conflict classification is from PR #12

**Verdict**: ✅ PASSES - No duplication, reuses PR #11/12 logic

### Q8: Does it duplicate transition/commit logic?

**Answer**: No. Reconciliation produces candidate; application commits via existing primitive.

**Evidence**:
- Reconciliation returns Value, not StateId
- Application calls `commit_transition()` to persist
- No second mutation pathway
- Uses PR #8's existing transition semantics

**Verdict**: ✅ PASSES - No duplication, uses PR #8 logic

### Q9: Does reconciliation mutate inputs?

**Answer**: No. Base, Left, Right remain immutable.

**Evidence**:
- API accepts immutable references
- No mutable paths to inputs
- Only outputs new state

**Verdict**: ✅ PASSES - Immutability preserved

### Q10: Does the design accidentally make FeltDB the policy owner?

**Answer**: No. Policy is explicitly application's responsibility.

**Evidence**:
- ResolutionPlan is application-supplied
- Strategy is application's logic
- FeltDB applies plan but doesn't generate it
- Classification is observational only

**Check**: Does contract require application to supply strategy?
- **Yes**. If not supplied, reconciliation fails
- Force explicit application input

**Verdict**: ✅ PASSES - FeltDB remains neutral, policy is application's

---

## 17. Final Verdict: Reconciliation Contract

### Authority Audit

| Criterion | Status | Evidence |
|-----------|--------|----------|
| No hidden winner | ✅ PASS | Classification observational, strategy explicit |
| No secret merge | ✅ PASS | Merge is optional strategy, not assumption |
| No authority influence | ✅ PASS | Authority is metadata, not decision logic |
| No temporal ordering | ✅ PASS | No implicit time-based decisions |
| Git independence | ✅ PASS | Pure semantic operation |
| No duplication | ✅ PASS | Uses existing PR #10, #11, #12, #8 primitives |
| No mutation | ✅ PASS | Immutability preserved throughout |
| No FeltDB policy | ✅ PASS | Application supplies all strategies |

### Readiness

**This Contract is READY FOR ACCEPTANCE if:**

1. ✅ The boundary between classification and reconciliation is clear
2. ✅ Mechanism and policy are separated
3. ✅ Three-way semantics are defined
4. ✅ Relationship-specific behavior is specified
5. ✅ Authority neutrality is proven
6. ✅ Immutability is preserved
7. ✅ Git independence is maintained
8. ✅ Commit boundary is explicit
9. ✅ Provenance requirements are documented
10. ✅ No speculative implementation is included
11. ✅ All architectural claims have supporting evidence

---

## 18. References

### Existing PRs Used as Foundation

- **PR #8**: State-Commit Transition Semantics
  - File: `src/state_store.rs` lines 477-501
  - API: `commit_transition(expected_parent, next_state)`

- **PR #10**: State Topology and Relationship Detection
  - File: `src/state_store.rs` lines 577-587
  - Functions: `relationship()`, `common_ancestor()`, `is_ancestor()`

- **PR #11**: Deterministic Semantic Diff
  - File: `src/state_store.rs` lines 603-641
  - API: `diff(left, right)` returns `StateDiff`

- **PR #12**: Conflict Classification
  - File: `src/state_store.rs` lines 642-702
  - API: `classify_conflicts(left, right)` returns `ConflictClassification`

### Key Data Structures

```rust
pub struct ConflictClassification {
    pub relationship: StateRelationship,
    pub base_state: Option<StateId>,
    pub left_changes: Vec<StateChange>,
    pub right_changes: Vec<StateChange>,
    pub path_conflicts: Vec<PathConflict>,
}

pub struct PathConflict {
    pub path: StatePath,
    pub left_change: StateChange,
    pub right_change: StateChange,
    pub conflict_type: ConflictType,  // Independent, Convergent, Conflict
}

pub enum StateRelationship {
    Identity,
    Ancestor,
    Descendant,
    Diverged,
    Unrelated,
}

pub struct ResolutionPlan {
    pub path_resolutions: BTreeMap<StatePath, Value>,
    pub auto_convergent_paths: Vec<StatePath>,
    pub auto_independent_paths: Vec<StatePath>,
}
```

---

## 19. Conclusion

This contract establishes that:

1. **Reconciliation is possible** using existing FeltDB primitives
2. **Mechanism is separate from policy** - FeltDB provides framework, applications provide choices
3. **Authority is preserved** - FeltDB makes no assumptions about who should win
4. **Immutability is maintained** - inputs never mutate, only new states created
5. **Boundaries are explicit** - classification → strategy → reconciliation → commitment are clear stages

The architectural boundary is solid. When implementation begins (Phase 2), the design ensures FeltDB remains a durable state system, not a policy engine.

