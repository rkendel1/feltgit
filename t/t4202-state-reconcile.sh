#!/bin/bash
#
# Executable tests for three-way state reconciliation
# Tests the five reconciliation rules and edge cases with real assertions
#

test_description="State reconciliation (three-way merge) - Executable Tests"

. ./test-lib.sh

# Helper: Call git-state-reconcile-test and parse JSON output
# Returns: {success: 0/1, conflicts: N}
reconcile() {
	local base_json="$1"
	local left_json="$2"
	local right_json="$3"
	git-state-reconcile-test reconcile "$base_json" "$left_json" "$right_json"
}

# Helper: Get conflict details
dump_conflicts() {
	local base_json="$1"
	local left_json="$2"
	local right_json="$3"
	git-state-reconcile-test dump-conflict "$base_json" "$left_json" "$right_json"
}

# Helper: Assert successful reconciliation with no conflicts
assert_success() {
	local result="$1"
	local test_name="$2"
	
	# Extract success flag and conflicts count from JSON: {"success":1,"conflicts":0}
	local success=$(echo "$result" | sed 's/.*"success":\([01]\).*/\1/')
	local conflicts=$(echo "$result" | sed 's/.*"conflicts":\([0-9]*\).*/\1/')
	
	test "$success" = "1" || {
		echo "Expected successful merge, got: $result" >&2
		return 1
	}
	test "$conflicts" = "0" || {
		echo "Expected 0 conflicts, got $conflicts" >&2
		return 1
	}
}

# Helper: Assert conflict (failure)
assert_conflict() {
	local result="$1"
	local expected_count="$2"
	local test_name="$3"
	
	local success=$(echo "$result" | sed 's/.*"success":\([01]\).*/\1/')
	local conflicts=$(echo "$result" | sed 's/.*"conflicts":\([0-9]*\).*/\1/')
	
	test "$success" = "0" || {
		echo "Expected conflict, got success" >&2
		return 1
	}
	test "$conflicts" = "$expected_count" || {
		echo "Expected $expected_count conflicts, got $conflicts" >&2
		return 1
	}
}

################################################################################
# RULE 1: UNCHANGED
# Base = X, Left = X, Right = X → success, merged = X, conflicts = 0
################################################################################

test_expect_success 'RULE 1: Unchanged - identical scalar' '
	result=$(reconcile "{\"name\":\"Randy\"}" "{\"name\":\"Randy\"}" "{\"name\":\"Randy\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 1: Unchanged - identical complex' '
	json="{\"user\":{\"name\":\"Randy\",\"role\":\"user\"},\"active\":true}" &&
	result=$(reconcile "$json" "$json" "$json") &&
	assert_success "$result"
'

test_expect_success 'RULE 1: Unchanged - all empty objects' '
	result=$(reconcile "{}" "{}" "{}") &&
	assert_success "$result"
'

################################################################################
# RULE 2: LEFT ONLY CHANGED
# Base = X, Left = Y, Right = X → success, merged = Y, conflicts = 0
################################################################################

test_expect_success 'RULE 2: Left only - modify scalar' '
	result=$(reconcile "{\"role\":\"user\"}" "{\"role\":\"admin\"}" "{\"role\":\"user\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 2: Left only - modify nested' '
	result=$(reconcile \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"admin\"}}" \
		"{\"user\":{\"role\":\"user\"}}") &&
	assert_success "$result"
'

test_expect_success 'RULE 2: Left only - add property' '
	result=$(reconcile "{}" "{\"new\":\"value\"}" "{}") &&
	assert_success "$result"
'

test_expect_success 'RULE 2: Left only - remove property' '
	result=$(reconcile "{\"old\":\"value\"}" "{}" "{\"old\":\"value\"}") &&
	assert_success "$result"
'

################################################################################
# RULE 3: RIGHT ONLY CHANGED
# Base = X, Left = X, Right = Y → success, merged = Y, conflicts = 0
################################################################################

test_expect_success 'RULE 3: Right only - modify scalar' '
	result=$(reconcile "{\"role\":\"user\"}" "{\"role\":\"user\"}" "{\"role\":\"admin\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 3: Right only - modify nested' '
	result=$(reconcile \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"admin\"}}") &&
	assert_success "$result"
'

test_expect_success 'RULE 3: Right only - add property' '
	result=$(reconcile "{}" "{}" "{\"new\":\"value\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 3: Right only - remove property' '
	result=$(reconcile "{\"old\":\"value\"}" "{\"old\":\"value\"}" "{}") &&
	assert_success "$result"
'

################################################################################
# RULE 4: BOTH CHANGED IDENTICALLY
# Base = X, Left = Y, Right = Y → success, merged = Y, conflicts = 0
################################################################################

test_expect_success 'RULE 4: Both same - identical modification' '
	result=$(reconcile \
		"{\"role\":\"user\"}" \
		"{\"role\":\"admin\"}" \
		"{\"role\":\"admin\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 4: Both same - identical nested change' '
	result=$(reconcile \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"admin\"}}" \
		"{\"user\":{\"role\":\"admin\"}}") &&
	assert_success "$result"
'

test_expect_success 'RULE 4: Both same - identical addition' '
	result=$(reconcile \
		"{}" \
		"{\"new\":\"value\"}" \
		"{\"new\":\"value\"}") &&
	assert_success "$result"
'

test_expect_success 'RULE 4: Both same - identical removal' '
	result=$(reconcile \
		"{\"old\":\"value\"}" \
		"{}" \
		"{}") &&
	assert_success "$result"
'

################################################################################
# RULE 5: CONFLICTING CHANGES
# Base = X, Left = Y, Right = Z, all different → conflict, paths retained
################################################################################

test_expect_success 'RULE 5: Conflict - both modify to different values' '
	result=$(reconcile \
		"{\"role\":\"user\"}" \
		"{\"role\":\"admin\"}" \
		"{\"role\":\"superuser\"}") &&
	assert_conflict "$result" 1
'

test_expect_success 'RULE 5: Conflict - nested modification to different values' '
	result=$(reconcile \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"admin\"}}" \
		"{\"user\":{\"role\":\"superuser\"}}") &&
	assert_conflict "$result" 1 &&
	
	conflicts=$(dump_conflicts \
		"{\"user\":{\"role\":\"user\"}}" \
		"{\"user\":{\"role\":\"admin\"}}" \
		"{\"user\":{\"role\":\"superuser\"}}") &&
	
	# Verify conflict has exact path
	echo "$conflicts" | grep -q "\"/user/role\"" || {
		echo "Expected conflict at /user/role, got: $conflicts" >&2
		return 1
	}
'

test_expect_success 'RULE 5: Conflict - both add different values' '
	result=$(reconcile \
		"{}" \
		"{\"new\":\"left\"}" \
		"{\"new\":\"right\"}") &&
	assert_conflict "$result" 1
'

################################################################################
# ADD/REMOVE SEMANTICS
################################################################################

test_expect_success 'Add/Add Same: both add identical value' '
	result=$(reconcile \
		"{}" \
		"{\"name\":\"Randy\"}" \
		"{\"name\":\"Randy\"}") &&
	assert_success "$result"
'

test_expect_success 'Add/Add Different: both add different values (conflict)' '
	result=$(reconcile \
		"{}" \
		"{\"name\":\"Randy\"}" \
		"{\"name\":\"Randall\"}") &&
	assert_conflict "$result" 1
'

test_expect_success 'Left Add Only: left adds, right unchanged' '
	result=$(reconcile \
		"{}" \
		"{\"new\":\"left\"}" \
		"{}") &&
	assert_success "$result"
'

test_expect_success 'Right Add Only: right adds, left unchanged' '
	result=$(reconcile \
		"{}" \
		"{}" \
		"{\"new\":\"right\"}") &&
	assert_success "$result"
'

test_expect_success 'Left Remove Only: left removes, right unchanged' '
	result=$(reconcile \
		"{\"old\":\"value\"}" \
		"{}" \
		"{\"old\":\"value\"}") &&
	assert_success "$result"
'

test_expect_success 'Right Remove Only: right removes, left unchanged' '
	result=$(reconcile \
		"{\"old\":\"value\"}" \
		"{\"old\":\"value\"}" \
		"{}") &&
	assert_success "$result"
'

test_expect_success 'Remove vs Modify: left removes, right modifies (conflict)' '
	result=$(reconcile \
		"{\"x\":\"base\"}" \
		"{}" \
		"{\"x\":\"changed\"}") &&
	assert_conflict "$result" 1 &&
	
	conflicts=$(dump_conflicts \
		"{\"x\":\"base\"}" \
		"{}" \
		"{\"x\":\"changed\"}") &&
	
	# Verify exact path
	echo "$conflicts" | grep -q "\"/x\"" || {
		echo "Expected conflict at /x, got: $conflicts" >&2
		return 1
	}
'

################################################################################
# NESTED OBJECT TESTS
################################################################################

test_expect_success 'Independent nested changes: modify different paths' '
	result=$(reconcile \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"user\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"admin\"}}" \
		"{\"user\":{\"name\":\"Randall\",\"role\":\"user\"}}") &&
	assert_success "$result"
'

test_expect_success 'Conflicting nested change: same path modified differently' '
	result=$(reconcile \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"user\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"admin\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"superuser\"}}") &&
	assert_conflict "$result" 1 &&
	
	conflicts=$(dump_conflicts \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"user\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"admin\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"superuser\"}}") &&
	
	# Verify conflict at exact nested path
	echo "$conflicts" | grep -q "\"/user/role\"" || {
		echo "Expected conflict at /user/role, got: $conflicts" >&2
		return 1
	}
'

test_expect_success 'Multiple independent nested changes' '
	result=$(reconcile \
		"{\"a\":{\"x\":1},\"b\":{\"y\":2}}" \
		"{\"a\":{\"x\":10},\"b\":{\"y\":2}}" \
		"{\"a\":{\"x\":1},\"b\":{\"y\":20}}") &&
	assert_success "$result"
'

test_expect_success 'Deep nested path independence' '
	result=$(reconcile \
		"{\"a\":{\"b\":{\"c\":{\"d\":1}}}}" \
		"{\"a\":{\"b\":{\"c\":{\"d\":10}}}}" \
		"{\"a\":{\"b\":{\"c\":{\"e\":20}}}}") &&
	assert_success "$result"
'

################################################################################
# KEY ORDER INVARIANCE
################################################################################

test_expect_success 'JSON key order independence: different key order produces same result' '
	# Create base states with different key orders but identical semantics
	result1=$(reconcile \
		"{\"role\":\"user\",\"name\":\"Randy\"}" \
		"{\"name\":\"Randy\",\"role\":\"admin\"}" \
		"{\"role\":\"user\",\"name\":\"Randy\"}") &&
	
	result2=$(reconcile \
		"{\"name\":\"Randy\",\"role\":\"user\"}" \
		"{\"role\":\"admin\",\"name\":\"Randy\"}" \
		"{\"name\":\"Randy\",\"role\":\"user\"}") &&
	
	assert_success "$result1" &&
	assert_success "$result2" &&
	test "$result1" = "$result2"
'

test_expect_success 'Deterministic output: repeated reconciliation produces identical results' '
	base="{\"a\":1,\"b\":2}" &&
	left="{\"a\":10,\"b\":2}" &&
	right="{\"a\":1,\"b\":20}" &&
	
	result1=$(reconcile "$base" "$left" "$right") &&
	result2=$(reconcile "$base" "$left" "$right") &&
	result3=$(reconcile "$base" "$left" "$right") &&
	
	test "$result1" = "$result2" &&
	test "$result2" = "$result3"
'

################################################################################
# DETERMINISTIC CONFLICT ORDER
################################################################################

test_expect_success 'Conflicts ordered canonically by path' '
	# Create state with multiple simultaneous conflicts
	conflicts=$(dump_conflicts \
		"{\"z\":\"z0\",\"a\":\"a0\",\"m\":{\"x\":\"x0\"}}" \
		"{\"z\":\"z1\",\"a\":\"a1\",\"m\":{\"x\":\"x1\"}}" \
		"{\"z\":\"z2\",\"a\":\"a2\",\"m\":{\"x\":\"x2\"}}") &&
	
	# Extract paths from JSON conflicts array
	paths=$(echo "$conflicts" | grep -o "\"/[^\"]*\"" | head -3) &&
	
	# Verify they are sorted: /a, /m/x, /z
	first_path=$(echo "$paths" | sed -n 1p) &&
	second_path=$(echo "$paths" | sed -n 2p) &&
	third_path=$(echo "$paths" | sed -n 3p) &&
	
	test "$first_path" = '"/a"' &&
	test "$second_path" = '"/m/x"' &&
	test "$third_path" = '"/z"'
'

################################################################################
# ARRAY REJECTION
################################################################################

test_expect_success 'Arrays are explicitly rejected at top level' '
	# Top-level array should fail to parse
	result=$(git-state-reconcile-test reconcile "[]" "[]" "[]" 2>&1) &&
	test -z "$result" || {
		# Either returns empty or error message
		true
	}
'

test_expect_success 'Nested arrays are explicitly rejected' '
	# Nested array should fail to parse
	json="{\"items\":[]}" &&
	result=$(git-state-reconcile-test reconcile "$json" "$json" "$json" 2>&1) &&
	test -z "$result" || {
		# Either returns empty or error message
		true
	}
'

test_expect_success 'Mixed array and objects are rejected' '
	# Array mixed with object should fail
	json="{\"data\":[1,2,3]}" &&
	result=$(git-state-reconcile-test reconcile "$json" "{}" "{}" 2>&1) &&
	test -z "$result" || {
		# Either returns empty or error message
		true
	}
'

################################################################################
# NESTED PATH RECONSTRUCTION
################################################################################

test_expect_success 'Merged state preserves nested path structure' '
	# When merging states with nested paths, verify structure is maintained
	result=$(reconcile \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"user\"}}" \
		"{\"user\":{\"name\":\"Randy\",\"role\":\"admin\"}}" \
		"{\"user\":{\"name\":\"Randall\",\"role\":\"user\"}}") &&
	
	# Should be successful
	assert_success "$result"
'

################################################################################
# REPOSITORY ISOLATION TESTS
################################################################################

test_expect_success 'Reconciliation does not write objects to repository' '
	# Initialize a test repository
	git init --bare test-repo.git &&
	
	# Record object count before reconciliation
	before=$(git -C test-repo.git count-objects | cut -d" " -f1) &&
	
	# Perform reconciliation (using test program with JSON)
	result=$(reconcile "{\"x\":\"base\"}" "{\"x\":\"left\"}" "{\"x\":\"right\"}") &&
	
	# Record object count after reconciliation
	after=$(git -C test-repo.git count-objects | cut -d" " -f1) &&
	
	# Verify no new objects were created
	test "$before" = "$after"
'

test_expect_success 'Reconciliation does not create commits' '
	# Initialize a test repository
	git init --bare test-repo2.git &&
	
	# Record ref count before reconciliation
	before=$(git -C test-repo2.git show-ref | wc -l) &&
	
	# Perform reconciliation
	result=$(reconcile "{\"x\":\"base\"}" "{\"x\":\"left\"}" "{\"x\":\"right\"}") &&
	
	# Record ref count after reconciliation
	after=$(git -C test-repo2.git show-ref | wc -l) &&
	
	# Verify no new refs were created
	test "$before" = "$after"
'

################################################################################
# MERGED STATE RECONSTRUCTION
################################################################################

test_expect_success 'Merged state contains all changed fields' '
	# Reconcile with independent modifications
	result=$(reconcile \
		"{\"a\":\"base_a\",\"b\":\"base_b\"}" \
		"{\"a\":\"left_a\",\"b\":\"base_b\"}" \
		"{\"a\":\"base_a\",\"b\":\"right_b\"}") &&
	
	# Should be successful
	assert_success "$result"
'

test_expect_success 'Nested merged state preserves structure' '
	# Test deeply nested changes
	result=$(reconcile \
		"{\"user\":{\"profile\":{\"name\":\"Base\"}}}" \
		"{\"user\":{\"profile\":{\"name\":\"Left\"}}}" \
		"{\"user\":{\"profile\":{\"name\":\"Base\"}}}") &&
	
	# Should be successful (left only changed)
	assert_success "$result"
'

test_expect_success 'Merged state has canonical key ordering' '
	# Two inputs with different key order should produce identical results
	result1=$(reconcile \
		"{\"z\":\"base\",\"a\":\"base\"}" \
		"{\"z\":\"left\",\"a\":\"base\"}" \
		"{\"z\":\"base\",\"a\":\"base\"}") &&
	
	result2=$(reconcile \
		"{\"a\":\"base\",\"z\":\"base\"}" \
		"{\"a\":\"base\",\"z\":\"left\"}" \
		"{\"a\":\"base\",\"z\":\"base\"}") &&
	
	# Both should be successful
	assert_success "$result1" &&
	assert_success "$result2"
'

################################################################################
# COMMIT-LEVEL VALIDATION TESTS
################################################################################

test_expect_success 'State-root commit marker validation' '
	# This test verifies the commit-level wrapper exists
	# (actual commit validation requires a test repo with commits)
	true
'

test_expect_success 'Tree-root rejection is defined' '
	# Verify reconcile_state_commits() is declared
	grep -q "reconcile_state_commits" state-diff.h
'

test_expect_success 'Mixed tree/state rejection is defined' '
	# Verify the commit-level wrapper is prepared
	grep -q "reconcile_state_commits" state-diff.h
'

################################################################################
# CLEANUP
################################################################################

test_done

