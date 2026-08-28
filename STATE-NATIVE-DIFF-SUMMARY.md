# State-Native Diff Experiment - Implementation Summary

This document summarizes the implementation of state-native diff for state-root commits (PR #3 of the FeltDB state-root experiment sequence).

## Overview

Building on PR #2 (State-Root Integrity), which established that commits can reference state blobs instead of trees, PR #3 implements the ability to diff state commits by comparing their state blob objects directly rather than tree contents.

The implementation provides:
- **State-to-state diffs**: Compare two state commits by diffing their state blobs
- **Tree-to-tree diffs**: Unchanged - continue to use existing tree diff logic
- **Tree-to-state transitions**: Show state blob as a new file when transitioning from tree to state
- **State-to-tree transitions**: Show tree as a new file when transitioning from state to tree
- **Root state commits**: Support showing diffs for root state commits

## Implementation Details

### 1. New Functions in commit.c/commit.h

**Added Function:**
```c
struct object_id *get_commit_state_oid(const struct commit *commit)
```

This function retrieves the state OID from a state commit, analogous to `get_commit_tree_oid()` for tree commits.

### 2. New Functions in tree-diff.c/diff.h

**Added Functions:**
```c
void diff_state_oid(const struct object_id *old_oid,
                   const struct object_id *new_oid,
                   const char *base, struct diff_options *opt)

void diff_root_state_oid(const struct object_id *new_oid,
                        const char *base, struct diff_options *opt)
```

These functions create diff pairs for state blobs:
- `diff_state_oid()`: Diffs two state blob OIDs, creating a filespec pair with path "state"
- `diff_root_state_oid()`: Shows a root state commit's state blob as a new file

Implementation approach:
1. Allocate two `diff_filespec` structures with path "state"
2. Use `fill_filespec()` to initialize them with state blob OIDs
3. Create a diff pair and queue it in `diff_queued_diff`
4. The normal diff processing pipeline handles output generation

### 3. Modified log_tree_diff() in log-tree.c

Enhanced to detect and handle state commits:

1. **Detect commit type**: Check `commit->is_state_commit` flag
2. **Get root OID**: Call `get_commit_state_oid()` for state commits, `get_commit_tree_oid()` for tree commits
3. **Root commits**: Call `diff_root_state_oid()` for root state commits
4. **Parent-child diffs**: For each parent, determine the diff function to call:
   - **Both state**: `diff_state_oid()` - compare state blobs
   - **Both tree**: `diff_tree_oid()` - unchanged existing behavior
   - **State parent to tree child**: `diff_root_tree_oid()` - show tree as new file
   - **Tree parent to state child**: `diff_state_oid(NULL, state_oid)` - show state as new file

## Key Design Decisions

### 1. State Blob Representation in Diff
State blobs are represented in diffs as a single "state" file. This allows the diff machinery to show the changes between state objects as if they were file content changes.

### 2. Transition Handling
When transitioning between tree and state commits:
- Tree → State: Show state blob as a newly added file (no old side)
- State → Tree: Show tree as a newly added file (no old side)

This reflects the semantic change that the root object type has changed, not just the content.

### 3. Normal Tree Diff Unchanged
All existing tree-to-tree diff behavior is preserved. State commits only affect the diff behavior when both commits are states or when transitioning between types.

### 4. Root Commit Handling
Root state commits are handled by `diff_root_state_oid()`, which treats the state blob as if it's being created from nothing (empty/null old side).

## Test Coverage

**File:** `t/t4201-diff-state.sh`

Tests included:
1. **State-to-state diff**: Verifies state blob changes are shown
2. **Root state commit diff**: Shows state blob creation
3. **Diff output verification**: Ensures "state" appears in diff output
4. **Tree-to-state transition**: Shows state as new file
5. **State-to-tree transition**: Shows tree as new file
6. **State commit history**: Shows chain of state commits

## Limitations and Future Work

### Current Limitations
1. **No conflict handling** - When tree and state parents exist, only first parent is used (existing Git behavior)
2. **State path hard-coded** - All state blobs appear as "state" in diffs (could be improved with metadata)
3. **No state validation in diff** - Assumes state OIDs refer to valid blob objects (fsck validates this)

### Planned Future Work (Future PRs)
1. **PR #4**: State reconciliation/merge - Merging state commits with application-aware strategies
2. **PR #5**: State transport - Optimizing transfer of state objects
3. **PR #6**: Concurrent authorities - Implementing multi-source state updates

## Backward Compatibility

- ✅ All tree-based diffs continue to work unchanged
- ✅ Tree commit history is unaffected
- ✅ Existing diff options and flags work with state commits
- ✅ No changes to commit storage format
- ✅ Optional feature - only activated for commits with `is_state_commit` flag

## Performance Considerations

- **State diff**: Single filespec pair per diff (efficient)
- **Tree diff**: Unchanged complexity
- **Memory**: Minimal overhead - only state commits use additional filespec pairs
- **No performance regression**: State commits don't affect tree commit processing

## Security Considerations

- No new security vulnerabilities introduced
- State blob OIDs are validated by fsck before appearing in diff
- Diff output formatting follows existing security practices
- No injection or traversal concerns

## Integration Points

The implementation integrates at the following points:

1. **commit.c/h**: New `get_commit_state_oid()` accessor
2. **tree-diff.c/diff.h**: New state diff functions
3. **log-tree.c**: Modified diff output generation logic
4. **diff system**: Uses existing filespec pair infrastructure

## Testing

To test state-native diff manually:

```bash
# Create a state blob
state_blob=$(echo '{"v": 1}' | git hash-object -w --stdin)

# Create a state commit
state_commit_text="state $state_blob
author Test <test@example.com> 1234567890 +0000
committer Test <test@example.com> 1234567890 +0000

state commit"
state_commit=$(echo "$state_commit_text" | git hash-object -w -t commit --stdin)

# Create another state commit
state_blob2=$(echo '{"v": 2}' | git hash-object -w --stdin)
state_commit2_text="state $state_blob2
parent $state_commit
author Test <test@example.com> 1234567890 +0000
committer Test <test@example.com> 1234567890 +0000

state commit 2"
state_commit2=$(echo "$state_commit2_text" | git hash-object -w -t commit --stdin)

# Create a ref
git update-ref refs/heads/state $state_commit2

# View diff between state commits
git log -p refs/heads/state
git diff $state_commit $state_commit2
```

## Compilation

Build with:
```bash
make commit.o tree-diff.o log-tree.o
```

Or build the full Git:
```bash
make
```

Run tests:
```bash
cd t
./t4201-diff-state.sh
```

## Conclusion

PR #3 implements state-native diff, enabling diff output for state commits by comparing their state blob objects. This builds directly on PR #2's state-root integrity foundation and provides the infrastructure for future PRs that require state-aware operations like merge and transport.

The implementation is minimal, focused, and maintains full backward compatibility with existing tree-based diffs.
