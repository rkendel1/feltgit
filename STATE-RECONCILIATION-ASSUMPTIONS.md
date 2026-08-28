# State Reconciliation Assumptions and Evidence

## Experiment Overview

This document describes PR #4 - the three-way reconciliation experiment for state-root commits.

Building on:
- **PR #2**: Established that Git commits can have integrity-validated state roots
- **PR #3**: Established that state-root commits can be compared using deterministic semantic deltas

This PR (#4) tests the next primitive: three-way reconciliation.

## Research Question

> "Given a common state ancestor and two state descendants, can their semantic changes be deterministically reconciled without inventing an authority or silently resolving conflicting changes?"

## Architecture

The state reconciliation engine operates independently from Git's existing tree-based merge machinery:

```
tree commits
    ↓
existing Git merge
    ↓
(unchanged)

state commits
    ↓
semantic state diff (PR #3)
    ↓
three-way state reconciliation (PR #4)
    ↓
merged state OR explicit conflicts
```

No changes are made to Git's normal tree merge behavior.

## Implementation

### Core Data Structures

- **StateConflict**: Represents a single conflict, containing:
  - `path`: The canonical semantic path where conflict occurred
  - `base_value`, `left_value`, `right_value`: The three values for inspection

- **StateReconcileResult**: Contains either:
  - `merged_state`: The reconciled state (if successful)
  - `conflicts`: Array of conflicts (if reconciliation failed)
  - `success`: Flag indicating success/failure status

### Reconciliation Algorithm

Input: Three state objects (base, left, right)

Process:
1. Flatten each state into canonical path-value pairs
2. Collect union of all paths
3. For each path, apply five reconciliation rules:

**RULE 1 — UNCHANGED**
```
If base[path] == left[path] == right[path]
Result: base[path]
Status: No conflict
```

**RULE 2 — LEFT ONLY CHANGED**
```
If base[path] == right[path] AND left[path] != base[path]
Result: left[path]
Status: No conflict
```

**RULE 3 — RIGHT ONLY CHANGED**
```
If base[path] == left[path] AND right[path] != base[path]
Result: right[path]
Status: No conflict
```

**RULE 4 — BOTH CHANGED TO SAME VALUE**
```
If left[path] == right[path] AND left[path] != base[path]
Result: left[path]
Status: No conflict
```

**RULE 5 — CONFLICTING CHANGES**
```
If left[path] != base[path] AND right[path] != base[path] AND left[path] != right[path]
Result: CONFLICT
Status: Must retain base/left/right values for manual inspection
```

### Key Features

- **Deterministic**: Output is identical for same inputs, regardless of:
  - JSON key ordering
  - Path traversal order (except explicit left/right distinction)
  - No timestamps, random IDs, or implicit tie-breakers

- **Canonical Paths**: Uses `/`-separated paths for nested objects:
  - `/user/name`
  - `/user/role`
  - Enables conflict detection at the semantic level, not just at parent-object level

- **Explicit Conflicts**: When incompatible changes occur:
  - No automatic winner selection
  - No "last-write-wins" semantics
  - Conflicts are returned as metadata separate from the state

- **No Authority**: The algorithm uses only:
  - The three states themselves
  - Semantic comparison of values
  - No external ordering, timestamps, or authority

## State Model

Supported JSON types:
- `null`
- `boolean`
- `number`
- `string`
- `object` (nested objects supported)

**Unsupported**: Arrays (explicitly rejected with error)

All JSON must be UTF-8 encoded, top-level object.

## Addition/Removal Semantics

Removal is treated as a real semantic change, not as "missing data."

### Addition Test Cases

1. **Both add same value**
   - Base: absent, Left: X, Right: X → no conflict

2. **Both add different values**
   - Base: absent, Left: X, Right: Y → CONFLICT

3. **Left adds, right unchanged**
   - Base: absent, Left: X, Right: absent → left value wins

4. **Right adds, left unchanged**
   - Base: absent, Left: absent, Right: Y → right value wins

5. **Left removes, right unchanged**
   - Base: X, Left: absent, Right: X → left removal wins

6. **Right removes, left unchanged**
   - Base: X, Left: X, Right: absent → right removal wins

7. **Remove vs modify conflict**
   - Base: X, Left: absent, Right: Y → CONFLICT

## Nested Object Example

**Input:**
```
Base:
  {"user": {"name": "Randy", "role": "user"}}

Left:
  {"user": {"name": "Randy", "role": "admin"}}

Right:
  {"user": {"name": "Randall", "role": "user"}}
```

**Reconciliation Process:**
- Path `/user/name`: base="Randy", left="Randy", right="Randall"
  - RULE 3 applies (right only changed) → result: "Randall"

- Path `/user/role`: base="user", left="admin", right="user"
  - RULE 2 applies (left only changed) → result: "admin"

**Output:**
```
{"user": {"name": "Randall", "role": "admin"}}
```

No conflict, because changes occurred at independent paths.

## Conflict Representation

Conflicts are **not** merged into the state document. Instead:

```c
struct state_conflicts {
    struct state_conflict {
        char *path;                    // "/user/role"
        struct state_value *base_value;    // "user"
        struct state_value *left_value;    // "admin"
        struct state_value *right_value;   // "superuser"
    } *items;
    size_t count;
};
```

This preserves the original state values for human inspection without contaminating the state with presentation syntax like `<<<<<<<`.

## Commit-Level Integration

The reconciliation operates on state blobs extracted from commits.

For a reconciliation request:
- Input: Three commit OIDs (base, left, right)
- Each commit is validated as a state-root commit
- The state object is extracted from each commit
- Reconciliation proceeds as described above

No mixing of tree and state roots is allowed.

## Output

### Successful Reconciliation
- Returns the merged state object
- `success = 1`
- No conflicts array

### Conflicted Reconciliation
- Returns explicit conflict array
- `success = 0`
- No merged state (caller must not treat as valid)

The operation is **pure** - it does not:
- Create Git commits
- Update refs
- Mutate the repository
- Modify working directory

## PROVEN Capabilities

✅ **Common-base state commits can be reconciled**
- Three-way merge logic is implemented and tested
- Base, left, and right state objects are correctly processed

✅ **Left-only changes are incorporated**
- RULE 2 correctly accepts left-side changes when right is unchanged

✅ **Right-only changes are incorporated**
- RULE 3 correctly accepts right-side changes when left is unchanged

✅ **Identical concurrent changes are reconciled**
- RULE 4 accepts both sides changing to the same value

✅ **Independent nested changes are reconciled**
- Paths are treated canonically, allowing independent changes at different nesting levels

✅ **Conflicting changes are detected**
- RULE 5 identifies when both sides change a path to different values

✅ **Conflicts retain base/left/right values**
- Conflict representation preserves all three values for inspection

✅ **No implicit conflict winner is selected**
- No timestamps, author preference, or random selection
- Conflicts are explicit or non-existent

✅ **Reconciliation is deterministic**
- Same inputs produce identical outputs
- JSON key order does not affect result
- Paths are canonically ordered
- No non-deterministic sources (random, time, iteration order)

✅ **Tree merge behavior remains untouched**
- No modifications to existing Git merge logic
- No changes to tree-commit handling
- Existing repositories work unchanged

## NOT PROVEN (Out of Scope)

The following are explicitly **not** addressed by this PR:

- ❌ **Automatic conflict resolution**
  - This PR only detects conflicts, not resolves them
  - Resolution policy is application-specific

- ❌ **CRDT semantics**
  - Not implementing operation-based CRDTs or vector clocks
  - This is a simpler semantic merge, not distributed consensus

- ❌ **Concurrent authority**
  - No distributed consensus, leases, or ordering
  - Base-left-right is provided by caller

- ❌ **Distributed reconciliation**
  - No network transport or replication in this PR
  - Pure local operation

- ❌ **Merge-base discovery**
  - Caller must provide base explicitly
  - Not integrating with Git's merge-base machinery

- ❌ **Arrays**
  - Explicitly unsupported and rejected
  - Would require separate reconciliation semantics

- ❌ **Schema-aware reconciliation**
  - No schema registry or type hints
  - Pure semantic value comparison

- ❌ **Performance/scaling**
  - Not optimized for large states or many conflicts
  - Not tested at scale

- ❌ **Persistence**
  - No storage layer
  - In-memory operation only

- ❌ **Production merge UX**
  - Conflict output is structural, not user-friendly
  - Application layer must format for humans

## Important Limitations

1. **Determinism Requirement**: The same three inputs must produce byte-for-byte identical output. This means:
   - JSON key order does not matter (canonical comparison)
   - Path order is canonical (sorted)
   - Conflict order is canonical (sorted by path)

2. **Nested Object Handling**: Reconciliation operates at the leaf-value level, not at parent-object level. This allows:
   - Independent changes to coexist
   - Conflicts only at the specific path level, not at parent

3. **No Automatic Merging**: When a conflict is detected, the merged state is not created:
   - `merged_state = NULL`
   - `success = 0`
   - Caller must handle manually

4. **No Authority Mechanism**: The reconciliation algorithm is neutral - it uses no:
   - Timestamps
   - Author identities
   - Machine IDs
   - Ordering of changes
   - Random selection

## Files Changed

- `state-diff.h`: Added reconciliation structures and functions
- `state-diff.c`: Implemented reconciliation algorithm
- `git-state-reconcile-test.c`: Added test program for direct function testing
- `t/t4202-state-reconcile.sh`: Added test cases for all 22 scenarios

## Test Coverage

The PR includes executable tests for:

1. ✓ Identical states
2. ✓ Left-only modification
3. ✓ Right-only modification
4. ✓ Both modify same path to same value
5. ✓ Conflicting scalar modification
6. ✓ Left-only addition
7. ✓ Right-only addition
8. ✓ Identical additions
9. ✓ Conflicting additions
10. ✓ Left-only removal
11. ✓ Right-only removal
12. ✓ Remove vs modify conflict
13. ✓ Independent nested changes
14. ✓ Conflicting nested change
15. ✓ Multiple independent changes
16. ✓ Deterministic output
17. ✓ JSON key-order independence
18. ✓ Invalid JSON rejection
19. ✓ Missing state object handling
20. ✓ Tree-root input rejection
21. ✓ Mixed-root inputs rejection
22. ✓ End-to-end commit-based reconciliation

## Conclusion

**The research question is ANSWERED: YES**

Application-state changes from two descendants of a common state ancestor **can be deterministically reconciled when changes are independent, while explicitly detecting rather than silently resolving conflicting changes.**

The evidence shows:
- Semantic-level reconciliation is possible without distributed consensus
- Conflicts can be explicitly detected without implicit tie-breakers
- The algorithm is deterministic and reproducible
- Existing Git tree-merge behavior is completely preserved

This establishes the foundation for stateful applications built on Git's integrity primitives.

PR #4 does NOT address conflict resolution policy, production merge UX, CRDT semantics, or distributed authority. Those are topics for future PRs.
