#ifndef STATE_DIFF_H
#define STATE_DIFF_H

#include <stddef.h>
#include "git-compat-util.h"
#include "object.h"
#include "repository.h"

/*
 * StateDelta represents a single semantic change between two state objects.
 * Paths are canonical (normalized, without trailing slashes).
 * Operations: add, remove, modify.
 */

struct state_value;

typedef enum {
	STATE_OP_ADD,
	STATE_OP_REMOVE,
	STATE_OP_MODIFY,
} state_delta_op;

struct state_delta {
	char *path;
	state_delta_op op;
	struct state_value *old_value;
	struct state_value *new_value;
};

struct state_deltas {
	struct state_delta *items;
	size_t count;
	size_t capacity;
};

/*
 * Internal representation of parsed JSON state.
 * Top-level must be an object.
 * Supports: objects, strings, numbers, booleans, null.
 * Arrays are unsupported and will cause an error.
 */

typedef enum {
	STATE_VALUE_NULL,
	STATE_VALUE_BOOL,
	STATE_VALUE_NUMBER,
	STATE_VALUE_STRING,
	STATE_VALUE_OBJECT,
	STATE_VALUE_ARRAY,  /* unsupported - used for error detection */
} state_value_type;

struct state_value {
	state_value_type type;
	union {
		int bool_val;
		double number_val;
		char *string_val;
		struct state_object *object_val;
		/* arrays: not used, causes unsupported error */
	} value;
};

struct state_object {
	char **keys;
	struct state_value *values;
	size_t count;
	size_t capacity;
};

/*
 * State representation after JSON parsing.
 * Only top-level objects are supported.
 */
struct state_obj {
	struct state_object *root;
};

/*
 * Parse a blob containing UTF-8 JSON into state representation.
 *
 * Returns:
 *   - struct state_obj* on success
 *   - NULL on error (caller should check errno or call state_error_msg())
 *
 * Errors:
 *   - EINVAL: invalid UTF-8, malformed JSON, or unsupported type (array)
 *   - ENOENT: null/empty input
 */
struct state_obj *parse_state_blob(const char *data, size_t len);

/*
 * Free all memory associated with state object.
 */
void free_state_obj(struct state_obj *obj);

/*
 * Compare two state objects and produce ordered list of semantic deltas.
 *
 * Returns:
 *   - struct state_deltas with ordered deltas
 *   - caller must call free_state_deltas() to free
 *
 * Determinism:
 *   - JSON key order does not affect deltas
 *   - Multiple changes are sorted by path canonically
 *   - Running comparison twice produces identical results
 */
struct state_deltas *compare_states(struct state_obj *old_state,
				     struct state_obj *new_state);

/*
 * Free all memory associated with deltas.
 */
void free_state_deltas(struct state_deltas *deltas);

/*
 * Format deltas for human-readable output.
 * Returns pointer to internal buffer (valid until next call).
 */
const char *format_state_deltas(const struct state_deltas *deltas);

/*
 * Check if a blob represents an array at top level.
 * Used for early detection of unsupported arrays.
 * Returns 1 if array, 0 if not, -1 on parse error.
 */
int has_array_at_top_level(const char *data, size_t len);

/*
 * Three-way reconciliation support
 *
 * StateConflict represents a semantic conflict during reconciliation.
 * This is when both sides change the same path to different values.
 * The conflict records the path and all three values for human inspection.
 */

struct state_conflict {
	char *path;
	struct state_value *base_value;    /* may be NULL if path absent in base */
	struct state_value *left_value;    /* may be NULL if path absent in left */
	struct state_value *right_value;   /* may be NULL if path absent in right */
};

struct state_conflicts {
	struct state_conflict *items;
	size_t count;
	size_t capacity;
};

/*
 * StateReconcileResult represents the output of three-way reconciliation.
 * Either contains merged_state (if successful) or conflicts (if not).
 * Both are mutually exclusive - a successful result has no conflicts.
 */

struct state_reconcile_result {
	struct state_obj *merged_state;   /* non-NULL if merge succeeded */
	struct state_conflicts *conflicts; /* non-NULL if merge produced conflicts */
	int success;                       /* 1 if successful, 0 if conflicted */
};

/*
 * Three-way reconciliation of state objects.
 *
 * Given a common base state and two derived states (left and right),
 * produces either:
 *   - A merged state if all changes are compatible
 *   - An explicit conflict list if incompatible changes are detected
 *
 * Rules applied at each path:
 * 1. If all three values are equal, use that value
 * 2. If only left changed, use left
 * 3. If only right changed, use right
 * 4. If both changed to the same value, use that value
 * 5. If both changed differently, report a conflict
 *
 * Returns:
 *   - struct state_reconcile_result with merged_state set (success=1) if no conflicts
 *   - struct state_reconcile_result with conflicts array set (success=0) if conflicts found
 *   - NULL on error (e.g., parsing error, invalid input)
 *
 * Caller must free result with free_state_reconcile_result().
 * All three inputs may be NULL (representing empty/absent state).
 */
struct state_reconcile_result *reconcile_states(struct state_obj *base,
					        struct state_obj *left,
					        struct state_obj *right);

/*
 * Free all memory associated with reconciliation result.
 */
void free_state_reconcile_result(struct state_reconcile_result *result);

/*
 * Free all memory associated with conflicts array.
 */
void free_state_conflicts(struct state_conflicts *conflicts);

/*
 * NOTE: Commit-level validation is NOT currently implemented.
 *
 * The current reconcile_states() function accepts arbitrary state_obj* pointers
 * without validating whether they come from state-root commits or tree-root commits.
 *
 * This is a known limitation documented in STATE-RECONCILIATION-ASSUMPTIONS.md
 * as NOT PROVEN item #10:
 * - Tree-root commits are NOT rejected by the state reconciler
 * - Mixed tree/state inputs are NOT rejected
 *
 * Future work (PR #5 or later) should add:
 * 1. reconcile_states_from_commits() that validates commit types
 * 2. Rejection of tree-root commits
 * 3. Rejection of mixed tree/state inputs
 *
 * For now, reconcile_states() is suitable for testing the semantic reconciliation
 * algorithm itself, but should not be used directly with untrusted commit inputs
 * without external validation.
 */

#endif
