# FSCK Assumptions - State-Root Integrity Experiment

This document inventories the fsck assumptions that were changed as part of the state-root integrity experiment.

## Background

Previously, git fsck assumed:
```
commit → tree
```

The state-root integrity experiment generalizes this to:
```
commit → root
         ├── tree
         └── state
```

## Assumption Changes

### 1. Commit Root Type
**Original assumption:** `commit must contain tree`

**Generalized assumption:** `commit must contain either tree or state`

**Implementation:** 
- Modified `fsck_commit()` in `fsck.c` to accept either "tree " or "state " prefix
- Added parsing logic to distinguish between the two root types
- Appropriate error messages for parsing failures of either type

**Impact:** Commits can now reference either a tree object or a state object as their root.

### 2. Tree Object Existence and Parsing
**Original assumption:** `tree object must exist and be parseable`

**New assumption for tree commits:** `tree object must exist and be parseable` (unchanged)

**New assumption for state commits:** `state root validates its own object`

**Implementation:**
- State commits store the state object OID in a new `maybe_state_oid` field
- `fsck_walk_commit()` validates that state objects:
  - Exist in the object database
  - Are of type OBJ_BLOB (not OBJ_TREE or other types)

**Impact:** State commits have stricter validation rules:
- The state object must be a blob
- The state object must exist
- Tree commits retain their original validation behavior

### 3. Parent Validation
**Original assumption:** `parent must exist`

**Assumption after changes:** `parent must exist` (unchanged)

**Implementation:** No changes to parent validation logic

**Impact:** Commits can have parents of either type (tree commits can have state commit parents and vice versa)

### 4. Commit Metadata
**Original assumption:** `author, committer, date fields must be valid`

**Assumption after changes:** `author, committer, date fields must be valid` (unchanged)

**Implementation:** No changes to metadata validation logic

**Impact:** Metadata validation remains the same for both tree and state commits

### 5. Object Reachability
**Original assumption:** `all referenced objects must be reachable`

**Assumption after changes:** `all referenced objects must be reachable` (unchanged)

**Implementation:** No changes to reachability validation

**Impact:** Both tree and state commits must have reachable root objects

## Error Messages

Three new fsck error types were introduced:

1. `MISSING_STATE` - Triggered when:
   - A state commit's state object OID field is missing
   - A state object referenced by a state commit doesn't exist in the object database

2. `BAD_STATE_SHA1` - Triggered when:
   - The SHA1 value on the "state" line is malformed

3. `BAD_STATE_TYPE` - Triggered when:
   - A state commit's root object exists but is not a blob

## Backward Compatibility

- All existing tree-rooted commits continue to work as before
- fsck will still validate tree commits using the original rules
- No changes to the commit object format for tree-rooted commits
- The experimental state commit format is opt-in and must be explicitly created

## Forward Compatibility

- The state commit format is designed to coexist with tree commits in the same repository
- Commits can have mixed parent types (a state commit can have a tree commit as parent and vice versa)
- The commit graph structure itself is neutral to root type
