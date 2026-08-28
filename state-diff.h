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

#endif
