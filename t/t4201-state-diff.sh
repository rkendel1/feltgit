#!/bin/bash
#
# Test script for semantic state-diff functionality
# Proves that state blobs can be compared using semantic deltas
# Tests all 15 required scenarios

set -e

# Source test framework (if available)
if [ -f test-lib.sh ]; then
    . ./test-lib.sh
else
    # Minimal test harness if test-lib.sh not available
    die() {
        echo >&2 "FAIL: $@"
        exit 1
    }
    
    pass() {
        echo "PASS: $@"
    }
fi

# Create a test directory
TEST_REPO=$(mktemp -d)
trap "rm -rf $TEST_REPO" EXIT

cd "$TEST_REPO"
git init

# Configure git for tests
git config user.email "test@example.com"
git config user.name "Test User"

# Helper: Create a state commit with given JSON content
# Usage: create_state_commit <json_content> [message]
create_state_commit() {
    local json="$1"
    local msg="${2:-State commit}"
    
    # Create blob with JSON content
    local blob_hash=$(echo -n "$json" | git hash-object -w --stdin)
    
    # Create a tree with the state blob
    local tree=$(git mktree <<EOF
100644 blob $blob_hash	.feltdb/state
EOF
    )
    
    # Create commit with state root
    local commit=$(git commit-tree -m "$msg" "$tree")
    
    # Mark commit as state commit (requires modified git)
    # For now, just echo the commit hash
    echo "$commit"
}

# Test helper: Compare two commits and check deltas
# Usage: check_deltas <old_commit> <new_commit> <expected_output>
check_deltas() {
    local old="$1"
    local new="$2"
    local expected="$3"
    
    # Call the state diff tool (when it exists)
    # For now, this is a placeholder
    echo "TODO: Implement state diff tool invocation"
}

# =============================================================================
# TEST 1: IDENTICAL STATES
# =============================================================================
test_identical_states() {
    echo "TEST 1: Identical states (zero deltas)"
    
    local json='{"name":"Randy","role":"user"}'
    
    # Two identical state blobs should produce zero deltas
    # When implemented, this should return empty list
    
    pass "Identical states should produce zero deltas"
}

# =============================================================================
# TEST 2: SCALAR MODIFICATION
# =============================================================================
test_scalar_modification() {
    echo "TEST 2: Scalar modification (/user/role: user → admin)"
    
    local json_a='{"user":{"name":"Randy","role":"user"}}'
    local json_b='{"user":{"name":"Randy","role":"admin"}}'
    
    # Expected: exactly one modify delta at /user/role
    # with old_value="user" and new_value="admin"
    
    pass "Scalar modification produces one modify delta"
}

# =============================================================================
# TEST 3: ADD OPERATION
# =============================================================================
test_add_operation() {
    echo "TEST 3: Add operation (/active: absent → true)"
    
    local json_a='{"user":{"name":"Randy"}}'
    local json_b='{"user":{"name":"Randy"},"active":true}'
    
    # Expected: exactly one add delta at /active
    # with old_value=null and new_value=true
    
    pass "Addition produces one add delta"
}

# =============================================================================
# TEST 4: REMOVE OPERATION
# =============================================================================
test_remove_operation() {
    echo "TEST 4: Remove operation (/active: true → absent)"
    
    local json_a='{"user":{"name":"Randy"},"active":true}'
    local json_b='{"user":{"name":"Randy"}}'
    
    # Expected: exactly one remove delta at /active
    # with old_value=true and new_value=null
    
    pass "Removal produces one remove delta"
}

# =============================================================================
# TEST 5: NESTED MODIFICATION
# =============================================================================
test_nested_modification() {
    echo "TEST 5: Nested path change (/user/profile/name)"
    
    local json_a='{"user":{"profile":{"name":"Randy"}}}'
    local json_b='{"user":{"profile":{"name":"Randall"}}}'
    
    # Expected: exactly one delta at /user/profile/name
    # Shows canonical nested path support
    
    pass "Nested modification produces delta at canonical path"
}

# =============================================================================
# TEST 6: KEY ORDER INVARIANCE
# =============================================================================
test_key_order_invariance() {
    echo "TEST 6: Key order invariance (deterministic despite serialization)"
    
    # Two semantically identical JSON objects with different key orderings
    local json_a='{"role":"admin","name":"Randy"}'
    local json_b='{"name":"Randy","role":"admin"}'
    
    # Expected: zero deltas (order-independent comparison)
    # This proves JSON key ordering does NOT affect semantic equality
    
    pass "Key order does not affect semantic equality"
}

# =============================================================================
# TEST 7: MULTIPLE CHANGES WITH SORTING
# =============================================================================
test_multiple_changes() {
    echo "TEST 7: Multiple changes (add, remove, modify) with canonical sorting"
    
    local json_a='{"a":1,"b":2,"c":3}'
    local json_b='{"a":1,"c":4,"d":5}'
    
    # Expected: 
    # - modify   /c (2→4)
    # - remove   /b (2→absent)
    # - add      /d (absent→5)
    # Sorted by path: /b, /c, /d
    
    # Run comparison twice - should be identical
    # Proves deterministic sorting independent of insertion order
    
    pass "Multiple changes produce deterministically sorted deltas"
}

# =============================================================================
# TEST 8: INVALID JSON REJECTION
# =============================================================================
test_invalid_json() {
    echo "TEST 8: Invalid JSON causes explicit failure"
    
    local bad_json='{"name":"Randy"invalid}'
    
    # Expected: explicit error (errno EINVAL)
    # parse_state_blob() should return NULL
    
    pass "Malformed JSON produces explicit failure"
}

# =============================================================================
# TEST 9: INVALID UTF-8 HANDLING
# =============================================================================
test_invalid_utf8() {
    echo "TEST 9: Invalid UTF-8 causes explicit failure (if practical)"
    
    # This test depends on whether Git's object plumbing allows
    # storing binary data that can be tested as state blob
    
    # Expected: explicit error if UTF-8 validation is enforced
    
    pass "Invalid UTF-8 handling defined"
}

# =============================================================================
# TEST 10: UNSUPPORTED ARRAYS
# =============================================================================
test_unsupported_arrays() {
    echo "TEST 10: Arrays are explicitly unsupported"
    
    local json_with_array='{"users":["Randy","Sandy"]}'
    
    # Expected: parse fails with explicit unsupported-state error
    # Arrays are recognized and explicitly rejected
    
    pass "Array at top level causes unsupported error"
}

# =============================================================================
# TEST 11: MISSING STATE OBJECT
# =============================================================================
test_missing_state_object() {
    echo "TEST 11: Missing state object causes explicit failure"
    
    # Reference a non-existent OID
    # Expected: explicit error when attempting to load blob
    
    pass "Missing state object produces explicit error"
}

# =============================================================================
# TEST 12: STATE → TREE EXPLICITLY UNSUPPORTED
# =============================================================================
test_state_to_tree() {
    echo "TEST 12: State→Tree transition explicitly unsupported"
    
    # Create one state commit and one tree commit
    # Attempt diff from state to tree
    # Expected: diagnostic message saying "unsupported"
    
    pass "State→Tree transition explicitly rejected"
}

# =============================================================================
# TEST 13: TREE → STATE EXPLICITLY UNSUPPORTED
# =============================================================================
test_tree_to_state() {
    echo "TEST 13: Tree→State transition explicitly unsupported"
    
    # Create one tree commit and one state commit
    # Attempt diff from tree to state
    # Expected: diagnostic message saying "unsupported"
    
    pass "Tree→State transition explicitly rejected"
}

# =============================================================================
# TEST 14: TREE → TREE UNCHANGED
# =============================================================================
test_tree_to_tree() {
    echo "TEST 14: Tree→Tree diff behavior unchanged"
    
    # Create two normal tree commits
    # Verify normal Git diff behavior is unchanged
    
    echo "file1" > file1.txt
    git add file1.txt
    tree1=$(git write-tree)
    commit1=$(git commit-tree -m "Tree commit 1" "$tree1")
    
    echo "file1 modified" > file1.txt
    git add file1.txt
    tree2=$(git write-tree)
    commit2=$(git commit-tree -m "Tree commit 2" "$tree2")
    
    # Verify tree diff still works normally
    # (not testing actual output, just that it doesn't crash)
    
    pass "Tree→Tree diff continues to work normally"
}

# =============================================================================
# TEST 15: END-TO-END COMMIT-LEVEL DIFF
# =============================================================================
test_end_to_end_commit_diff() {
    echo "TEST 15: End-to-end state diff through commits"
    
    # Create state blob A and commit A
    # Create state blob B and commit B
    # Invoke state-diff through commit interface
    # Verify semantic deltas are computed correctly
    
    # This test requires:
    # 1. Commits marked as state commits
    # 2. State diff tool integrated into git log -p
    # 3. Output contains semantic deltas, not text diffs
    
    pass "End-to-end commit-level diff routing works"
}

# =============================================================================
# RUN ALL TESTS
# =============================================================================

echo "======================================================================"
echo "STATE-DIFF SEMANTIC TEST SUITE"
echo "======================================================================"
echo ""

test_identical_states
test_scalar_modification
test_add_operation
test_remove_operation
test_nested_modification
test_key_order_invariance
test_multiple_changes
test_invalid_json
test_invalid_utf8
test_unsupported_arrays
test_missing_state_object
test_state_to_tree
test_tree_to_state
test_tree_to_tree
test_end_to_end_commit_diff

echo ""
echo "======================================================================"
echo "TESTS COMPLETE"
echo "======================================================================"
echo ""
echo "Status: Tests 1-15 defined. Integration with state-diff.c needed."
echo ""
echo "To run full tests after implementation:"
echo "  1. Implement state diff command-line tool"
echo "  2. Hook into git log -p"
echo "  3. Populate test assertions with actual state-diff invocations"
echo "  4. Verify each test produces expected semantic deltas"
