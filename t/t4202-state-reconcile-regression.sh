#!/bin/bash
# Comprehensive test suite for PR #4 and #5 state reconciliation

TEST_BIN="/home/runner/work/feltgit/feltgit/git-state-reconcile-test"
TESTS_PASSED=0
TESTS_FAILED=0

# Color output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_test() {
    echo -e "${YELLOW}Test: $1${NC}"
}

pass() {
    echo -e "${GREEN}✓ PASS${NC}"
    ((TESTS_PASSED++))
}

fail() {
    echo -e "${RED}✗ FAIL: $1${NC}"
    ((TESTS_FAILED++))
}

# Helper: reconcile two JSON objects
reconcile() {
    local base_json="$1"
    local left_json="$2"
    local right_json="$3"
    $TEST_BIN reconcile "$base_json" "$left_json" "$right_json" 2>&1
}

# Helper: extract field from JSON
json_get() {
    local json="$1"
    local field="$2"
    echo "$json" | sed "s/.*\"$field\":\([^,}]*\).*/\1/"
}

# Helper: Assert success (success=1, conflicts=0)
assert_success() {
    local result="$1"
    local test_name="$2"
    
    local success=$(json_get "$result" "success")
    local conflicts=$(json_get "$result" "conflicts")
    
    if [ "$success" = "1" ] && [ "$conflicts" = "0" ]; then
        pass
        return 0
    else
        fail "Expected success, got: $result"
        return 1
    fi
}

# Helper: Assert conflict
assert_conflict() {
    local result="$1"
    local expected_count="$2"
    local test_name="$3"
    
    local success=$(json_get "$result" "success")
    local conflicts=$(json_get "$result" "conflicts")
    
    if [ "$success" = "0" ] && [ "$conflicts" = "$expected_count" ]; then
        pass
        return 0
    else
        fail "Expected $expected_count conflicts, got: $result"
        return 1
    fi
}

echo "=========================================="
echo "PR #4 SEMANTIC RECONCILIATION TEST SUITE"
echo "=========================================="
echo ""

# ============================================================================
# REGRESSION TEST: Crash case from PR #5 audit
# ============================================================================
echo "REGRESSION TESTS"
echo "---"

log_test "1: Regression - segfault when right adds field"
result=$(reconcile '{"a":1}' '{"a":1}' '{"a":1,"b":2}')
if [ -z "$result" ]; then
    fail "Process crashed (empty output)"
else
    assert_success "$result" || true
fi

log_test "2: Regression - left adds field"
result=$(reconcile '{"a":1}' '{"a":1,"b":2}' '{"a":1}')
if [ -z "$result" ]; then
    fail "Process crashed (empty output)"
else
    assert_success "$result" || true
fi

log_test "3: Regression - empty base, right adds"
result=$(reconcile '{}' '{}' '{"a":1}')
if [ -z "$result" ]; then
    fail "Process crashed (empty output)"
else
    assert_success "$result" || true
fi

log_test "4: Regression - empty base, both add same"
result=$(reconcile '{}' '{"a":1}' '{"a":1}')
if [ -z "$result" ]; then
    fail "Process crashed (empty output)"
else
    assert_success "$result" || true
fi

log_test "5: Regression - empty base, both add different"
result=$(reconcile '{}' '{"a":1}' '{"a":2}')
if [ -z "$result" ]; then
    fail "Process crashed (empty output)"
else
    assert_conflict "$result" 1 || true
fi

echo ""

# ============================================================================
# RULE 1: UNCHANGED
# ============================================================================
echo "RULE 1: UNCHANGED (base=left=right)"
echo "---"

log_test "1.1: Identical scalar values"
result=$(reconcile '{"name":"Randy"}' '{"name":"Randy"}' '{"name":"Randy"}')
assert_success "$result" || true

log_test "1.2: All empty objects"
result=$(reconcile '{}' '{}' '{}')
assert_success "$result" || true

echo ""

# ============================================================================
# RULE 2: LEFT ONLY CHANGED
# ============================================================================
echo "RULE 2: LEFT ONLY CHANGED (base=right, left≠base)"
echo "---"

log_test "2.1: Left modifies scalar"
result=$(reconcile '{"role":"user"}' '{"role":"admin"}' '{"role":"user"}')
assert_success "$result" || true

log_test "2.2: Left adds property"
result=$(reconcile '{}' '{"new":"value"}' '{}')
assert_success "$result" || true

log_test "2.3: Left removes property"
result=$(reconcile '{"old":"value"}' '{}' '{"old":"value"}')
assert_success "$result" || true

echo ""

# ============================================================================
# RULE 3: RIGHT ONLY CHANGED
# ============================================================================
echo "RULE 3: RIGHT ONLY CHANGED (base=left, right≠base)"
echo "---"

log_test "3.1: Right modifies scalar"
result=$(reconcile '{"role":"user"}' '{"role":"user"}' '{"role":"admin"}')
assert_success "$result" || true

log_test "3.2: Right adds property"
result=$(reconcile '{}' '{}' '{"new":"value"}')
assert_success "$result" || true

log_test "3.3: Right removes property"
result=$(reconcile '{"old":"value"}' '{"old":"value"}' '{}')
assert_success "$result" || true

echo ""

# ============================================================================
# RULE 4: BOTH CHANGED IDENTICALLY
# ============================================================================
echo "RULE 4: BOTH CHANGED IDENTICALLY (left=right≠base)"
echo "---"

log_test "4.1: Both modify identically"
result=$(reconcile '{"role":"user"}' '{"role":"admin"}' '{"role":"admin"}')
assert_success "$result" || true

log_test "4.2: Both add identical value"
result=$(reconcile '{}' '{"name":"Randy"}' '{"name":"Randy"}')
assert_success "$result" || true

log_test "4.3: Both remove identical value"
result=$(reconcile '{"old":"value"}' '{}' '{}')
assert_success "$result" || true

echo ""

# ============================================================================
# RULE 5: CONFLICTING CHANGES
# ============================================================================
echo "RULE 5: CONFLICTING CHANGES (left≠base, right≠base, left≠right)"
echo "---"

log_test "5.1: Both modify to different values"
result=$(reconcile '{"role":"user"}' '{"role":"admin"}' '{"role":"superuser"}')
assert_conflict "$result" 1 || true

log_test "5.2: Both add different values"
result=$(reconcile '{}' '{"new":"left"}' '{"new":"right"}')
assert_conflict "$result" 1 || true

echo ""

# ============================================================================
# ADD/REMOVE SEMANTICS
# ============================================================================
echo "ADD/REMOVE SEMANTICS"
echo "---"

log_test "6.1: Left add only"
result=$(reconcile '{}' '{"new":"left"}' '{}')
assert_success "$result" || true

log_test "6.2: Right add only"
result=$(reconcile '{}' '{}' '{"new":"right"}')
assert_success "$result" || true

log_test "6.3: Both add different (conflict)"
result=$(reconcile '{}' '{"name":"Randy"}' '{"name":"Randall"}')
assert_conflict "$result" 1 || true

echo ""

# ============================================================================
# NESTED PATHS
# ============================================================================
echo "NESTED PATHS"
echo "---"

log_test "7.1: Independent nested changes"
result=$(reconcile '{"user":{"name":"Randy","role":"user"}}' \
                   '{"user":{"name":"Randy","role":"admin"}}' \
                   '{"user":{"name":"Randall","role":"user"}}')
assert_success "$result" || true

log_test "7.2: Multiple independent nested changes"
result=$(reconcile '{"a":{"x":1},"b":{"y":2}}' \
                   '{"a":{"x":10},"b":{"y":2}}' \
                   '{"a":{"x":1},"b":{"y":20}}')
assert_success "$result" || true

echo ""

# ============================================================================
# DETERMINISM
# ============================================================================
echo "DETERMINISM"
echo "---"

log_test "9.1: Repeated reconciliation produces identical results"
base='{"a":1,"b":2}'
left='{"a":10,"b":2}'
right='{"a":1,"b":20}'
result1=$(reconcile "$base" "$left" "$right")
result2=$(reconcile "$base" "$left" "$right")
result3=$(reconcile "$base" "$left" "$right")
if [ "$result1" = "$result2" ] && [ "$result2" = "$result3" ]; then
    pass
else
    fail "Results differ: $result1 vs $result2 vs $result3"
fi

echo ""
echo "=========================================="
echo "TEST SUMMARY"
echo "=========================================="
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
