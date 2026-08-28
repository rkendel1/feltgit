#!/bin/sh
#
# Copyright (C) 2005 Rene Scharfe
#

test_description='git commit-tree options test

This test checks that git commit-tree can create a specific commit
object by defining all environment variables that it understands.

Also make sure that command line parser understands the normal
"flags first and then non flag arguments" command line.
'

. ./test-lib.sh

test_expect_success 'test preparation: write empty tree' '
	cat >expected <<-EOF &&
	tree $EMPTY_TREE
	author Author Name <author@email> 1117148400 +0000
	committer Committer Name <committer@email> 1117150200 +0000

	comment text
	EOF
	git write-tree >treeid
'

test_expect_success 'construct commit' '
	echo comment text |
	GIT_AUTHOR_NAME="Author Name" \
	GIT_AUTHOR_EMAIL="author@email" \
	GIT_AUTHOR_DATE="2005-05-26 23:00" \
	GIT_COMMITTER_NAME="Committer Name" \
	GIT_COMMITTER_EMAIL="committer@email" \
	GIT_COMMITTER_DATE="2005-05-26 23:30" \
	TZ=GMT git commit-tree $(cat treeid) >commitid 2>/dev/null
'

test_expect_success 'read commit' '
	git cat-file commit $(cat commitid) >commit
'

test_expect_success 'compare commit' '
	test_cmp expected commit
'

test_expect_success 'flags and then non flags' '
	test_tick &&
	echo comment text |
	git commit-tree $(cat treeid) >commitid &&
	echo comment text |
	git commit-tree $(cat treeid) -p $(cat commitid) >childid-1 &&
	echo comment text |
	git commit-tree -p $(cat commitid) $(cat treeid) >childid-2 &&
	test_cmp childid-1 childid-2 &&
	git commit-tree $(cat treeid) -m foo >childid-3 &&
	git commit-tree -m foo $(cat treeid) >childid-4 &&
	test_cmp childid-3 childid-4
'

test_expect_success 'create experimental state commit from blob root' '
	printf "{\"counter\":1,\"name\":\"Randy\"}" |
	git hash-object -w --stdin >stateoid &&
	echo state comment |
	git commit-tree --experimental-state $(cat stateoid) -p $(cat commitid) >statecommitid
'

test_expect_success 'parse and recover state root from state commit' '
	git cat-file commit $(cat statecommitid) >statecommit &&
	echo "state $(cat stateoid)" >expect-state-line &&
	head -n 1 statecommit >actual-state-line &&
	test_cmp expect-state-line actual-state-line &&
	grep "^parent $(cat commitid)$" statecommit
'

test_expect_success 'state commit is traversable and can be referenced' '
	git update-ref refs/heads/state-experiment $(cat statecommitid) &&
	git rev-list --max-count=2 refs/heads/state-experiment >walk &&
	echo $(cat statecommitid) >expect-walk &&
	echo $(cat commitid) >>expect-walk &&
	test_cmp expect-walk walk &&
	git log --oneline -2 refs/heads/state-experiment >log-walk &&
	grep $(cat statecommitid | cut -c1-7) log-walk &&
	grep $(cat commitid | cut -c1-7) log-walk
'

test_expect_success 'ordinary tree commits still parse as tree commits' '
	git cat-file commit $(cat childid-4) >treecommit &&
	echo "tree $(cat treeid)" >expect-tree-line &&
	head -n 1 treecommit >actual-tree-line &&
	test_cmp expect-tree-line actual-tree-line
'

test_expect_success 'experimental state mode rejects non-blob root' '
	test_must_fail git commit-tree --experimental-state $(cat treeid)
'

test_expect_success 'fsck still enforces tree-root commit invariant' '
	test_must_fail git fsck --strict
'

test_done
