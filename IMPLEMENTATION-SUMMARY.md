# State-Root Integrity Experiment - Implementation Summary

This document summarizes the implementation of the state-root integrity experiment for the FeltDB project.

## Overview

The experiment generalizes Git's commit integrity model from:
```
commit → tree
```

to:

```
commit → root
         ├── tree
         └── state
```

This allows commits to reference either a tree object (as in normal Git) or a state blob (for application-state versioning).

## Implementation Details

### 1. FSck Enhancement (fsck.h, fsck.c)

**New Error Types Added:**
- `MISSING_STATE` - Triggered when a state commit's state object doesn't exist
- `BAD_STATE_SHA1` - Triggered when the SHA1 on the "state" line is malformed
- `BAD_STATE_TYPE` - Triggered when a state commit's root object is not a blob

**Modified Functions:**
- `fsck_commit()` - Now accepts either "tree " or "state " prefix
  - Parses the root line and stores the root OID
  - Reports appropriate errors for malformed state lines
  - Maintains backward compatibility with tree commits

- `fsck_walk_commit()` - Now handles both tree and state roots
  - For state commits:
    - Validates that the state object OID exists
    - Validates that the state object is a blob
    - Walks the state object through the validation callback
  - For tree commits:
    - Maintains original validation behavior
    - No changes to existing fsck logic

### 2. Commit Structure Enhancement (commit.h, commit.c)

**New Fields Added to `struct commit`:**
- `is_state_commit` - Bit flag indicating whether this is a state commit
- `maybe_state_oid` - Pointer to the state object OID (for state commits only)

**Modified Functions:**
- `parse_commit_buffer()` - Enhanced to:
  - Detect whether the root is a "tree" or "state"
  - Set `is_state_commit` flag appropriately
  - For tree commits: Store the tree object as before
  - For state commits: Store the state object OID
  - Provide appropriate error messages for state root parsing

- `release_commit_memory()` - Enhanced to:
  - Free the `maybe_state_oid` field when releasing memory
  - Maintain backward compatibility with tree commits

### 3. Test Coverage (t/t1450-fsck.sh)

**Seven Comprehensive Tests Added:**

1. **State Commit with Valid State Blob** - Proves that state commits with valid state blobs pass fsck
   - Creates a JSON blob as state
   - Creates a state commit pointing to it
   - Verifies fsck passes

2. **Normal Tree Commit Still Works** - Proves backward compatibility
   - Creates a normal tree commit
   - Verifies fsck passes (unchanged behavior)

3. **Missing State Object Fails** - Proves state object existence validation
   - Creates a state commit
   - Removes the state blob
   - Verifies fsck fails with error

4. **State Pointing to Tree Fails** - Proves state roots must be blobs
   - Creates a state commit pointing to a tree object
   - Verifies fsck fails with BAD_STATE_TYPE error

5. **Nonexistent State OID Fails** - Proves validation of state object references
   - Creates a state commit with fake OID
   - Verifies fsck fails

6. **State Commit with Tree Parent** - Proves commit graph is indifferent to root type
   - Creates a tree commit
   - Creates a state commit with the tree commit as parent
   - Verifies fsck passes

7. **Tree Commit with State Parent** - Proves bidirectional parent compatibility
   - Creates a state commit
   - Creates a tree commit with the state commit as parent
   - Verifies fsck passes

### 4. Documentation

**FSCK-ASSUMPTIONS.md** - Detailed inventory of fsck assumption changes:
- Lists all assumptions that were changed
- Explains the rationale for each change
- Documents error messages and validation rules
- Discusses backward compatibility and forward compatibility

## Key Design Decisions

### 1. State Objects Must Be Blobs
We require that state root objects must be blob objects, not trees. This enforces the semantic difference between:
- **Tree commits**: Store directory structure (filesystem versioning)
- **State commits**: Store application state (state versioning)

### 2. Backward Compatibility
All existing tree-rooted commits continue to work without modification. The experimental state commit format is opt-in and must be explicitly created.

### 3. Mixed Commit Graphs
The commit graph is indifferent to root type - a state commit can have tree commits as parents and vice versa. This allows gradual adoption of state commits in existing repositories.

### 4. Error Validation
The implementation provides specific error messages for different failure modes:
- Parsing errors are reported as they are detected
- Object existence is validated
- Object type is validated for state roots

## Limitations and Future Work

### Current Limitations
1. **Diff not implemented** - State-native diff will be in a future PR
2. **Merge not implemented** - State reconciliation/merge will be in a future PR  
3. **Transport not implemented** - State transport will be in a future PR
4. **No state object creation tools** - State commits must be created manually via `git hash-object`

### Planned Future Work
According to the PR sequence:
1. **PR 2 (current)**: State-root integrity ✓
2. **PR 3**: State-native diff (will replace tree diff with state delta)
3. **PR 4**: State reconciliation / merge
4. **PR 5**: State transport
5. **PR 6**: Concurrent authorities

## Testing State Commits Manually

To create and test state commits manually:

```bash
# Create a state blob
state_blob=$(echo '{"key": "value"}' | git hash-object -w --stdin)

# Create a state commit
commit_text="state $state_blob
author Test <test@example.com> 1234567890 +0000
committer Test <test@example.com> 1234567890 +0000

state commit"

state_commit=$(echo "$commit_text" | git hash-object -w -t commit --stdin)

# Create a ref to the state commit
git update-ref refs/heads/state $state_commit

# Verify fsck passes
git fsck
```

## Compilation and Testing

**Build:**
```bash
make commit.o fsck.o
```

**Run Tests:**
The test cases in `t/t1450-fsck.sh` cover all the requirements. They can be run with:
```bash
cd t
./t1450-fsck.sh
```

## Security Considerations

- No new security vulnerabilities introduced
- All memory allocations properly freed
- NULL pointer checks in place
- Input validation for object types
- Existing fsck security model maintained

## Performance Impact

The changes have minimal performance impact:
- New field added to commit struct uses bit-packing (1 bit)
- Pointer added for state OID (only for state commits)
- Additional validation only for state commits
- Tree commits have unchanged performance path
