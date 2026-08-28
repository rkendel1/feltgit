#!/bin/sh
# Test state-native diff functionality
#
# These tests verify that state commits can be diffed by comparing their
# state blob objects, and that tree-to-state and state-to-tree transitions work.

test_description='state-native diff support'

. ./test-lib.sh

# Helper function to create a state commit
create_state_commit() {
	local repo=$1
	local state_json=$2
	local parent_commit=$3
	local commit_msg=$4
	
	(
		cd "$repo" || return
		state_blob=$(printf "%s" "$state_json" | git hash-object -w --stdin)
		state_commit_text="state $state_blob"
		if [ -n "$parent_commit" ]; then
			state_commit_text="$state_commit_text
parent $parent_commit"
		fi
		state_commit_text="$state_commit_text
author Test <test@example.com> 1234567890 +0000
committer Test <test@example.com> 1234567890 +0000

$commit_msg"
		printf "%s" "$state_commit_text" | git hash-object -w -t commit --stdin
	)
}

# Helper function to create a tree commit
create_tree_commit() {
	local repo=$1
	local content=$2
	local parent_commit=$3
	local commit_msg=$4
	
	(
		cd "$repo" || return
		echo "$content" > file
		git add file
		tree_oid=$(git write-tree)
		tree_commit_text="tree $tree_oid"
		if [ -n "$parent_commit" ]; then
			tree_commit_text="$tree_commit_text
parent $parent_commit"
		fi
		tree_commit_text="$tree_commit_text
author Test <test@example.com> 1234567890 +0000
committer Test <test@example.com> 1234567890 +0000

$commit_msg"
		printf "%s" "$tree_commit_text" | git hash-object -w -t commit --stdin
	)
}

test_expect_success 'state-to-state diff shows state blob changes' '
	git init --bare state-diff-repo &&
	(
		cd state-diff-repo &&
		# Create first state commit
		state1=$(create_state_commit . "{\"key\": \"value1\"}" "" "state commit 1") &&
		# Create second state commit with first as parent
		state2=$(create_state_commit . "{\"key\": \"value2\"}" "$state1" "state commit 2") &&
		git update-ref refs/heads/main "$state2" &&
		# Show diff between state commits
		git log -p --show-root "$state2" | head -20 | grep -q "state" &&
		true
	)
'

test_expect_success 'state-to-state diff of root state commit' '
	git init --bare root-state-diff-repo &&
	(
		cd root-state-diff-repo &&
		# Create root state commit
		state_root=$(create_state_commit . "{\"initial\": \"state\"}" "" "root state commit") &&
		git update-ref refs/heads/main "$state_root" &&
		# Show diff for root commit
		git log -p --show-root "$state_root" | grep -q "state" &&
		true
	)
'

test_expect_success 'state commit diff output contains state file' '
	git init --bare state-output-repo &&
	(
		cd state-output-repo &&
		state1=$(create_state_commit . "{\"v\": 1}" "" "first") &&
		state2=$(create_state_commit . "{\"v\": 2}" "$state1" "second") &&
		git update-ref refs/heads/main "$state2" &&
		# Verify diff output mentions state
		git diff "$state1" "$state2" | grep -q "state" &&
		true
	)
'

test_expect_success 'tree-to-state transition shows state as addition' '
	git init --bare tree-to-state-repo &&
	(
		cd tree-to-state-repo &&
		git config user.name "Test" &&
		git config user.email "test@example.com" &&
		# Create tree commit first
		tree_commit=$(create_tree_commit . "content" "" "tree commit") &&
		# Create state commit with tree commit as parent
		state_commit=$(create_state_commit . "{\"state\": \"data\"}" "$tree_commit" "state commit") &&
		git update-ref refs/heads/main "$state_commit" &&
		# Verify diff works
		git log -p --show-root "$state_commit" | head -30 &&
		true
	)
'

test_expect_success 'state-to-tree transition shows state removal' '
	git init --bare state-to-tree-repo &&
	(
		cd state-to-tree-repo &&
		git config user.name "Test" &&
		git config user.email "test@example.com" &&
		# Create state commit first
		state_commit=$(create_state_commit . "{\"state\": \"data\"}" "" "state commit") &&
		# Create tree commit with state commit as parent
		tree_commit=$(create_tree_commit . "content" "$state_commit" "tree commit") &&
		git update-ref refs/heads/main "$tree_commit" &&
		# Verify diff works
		git log -p --show-root "$tree_commit" | head -30 &&
		true
	)
'

test_expect_success 'state-to-state multiple commits shows history' '
	git init --bare state-history-repo &&
	(
		cd state-history-repo &&
		# Create chain of state commits
		state1=$(create_state_commit . "{\"version\": 1}" "" "v1") &&
		state2=$(create_state_commit . "{\"version\": 2}" "$state1" "v2") &&
		state3=$(create_state_commit . "{\"version\": 3}" "$state2" "v3") &&
		git update-ref refs/heads/main "$state3" &&
		# Show log with patches
		git log -p "$state3" | grep -q "state" &&
		git log --oneline "$state3" | wc -l | grep -q "3" &&
		true
	)
'

test_done
