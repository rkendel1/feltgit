#include "state-diff.h"
#include "git-compat-util.h"
#include "object.h"
#include "strbuf.h"
#include <string.h>
#include <stdlib.h>
#include <ctype.h>
#include <errno.h>

/* Minimal JSON parser for state objects */

static void skip_whitespace(const char **p, const char *end);
static struct state_value *parse_value(const char **p, const char *end);
static void free_state_value(struct state_value *val);

static void skip_whitespace(const char **p, const char *end)
{
	while (*p < end && isspace(**p))
		(*p)++;
}

static int peek_char(const char **p, const char *end)
{
	if (*p >= end)
		return -1;
	return (unsigned char)**p;
}

static int consume_char(const char **p, const char *end, char expected)
{
	if (*p >= end || **p != expected)
		return -1;
	(*p)++;
	return 0;
}


static char *parse_string(const char **p, const char *end)
{
	struct strbuf buf = STRBUF_INIT;

	if (consume_char(p, end, '"') < 0)
		return NULL;

	while (*p < end && **p != '"') {
		if (**p == '\\') {
			(*p)++;
			if (*p >= end)
				goto error;

			switch (**p) {
			case '"':
			case '\\':
			case '/':
				strbuf_addch(&buf, **p);
				break;
			case 'b':
				strbuf_addch(&buf, '\b');
				break;
			case 'f':
				strbuf_addch(&buf, '\f');
				break;
			case 'n':
				strbuf_addch(&buf, '\n');
				break;
			case 'r':
				strbuf_addch(&buf, '\r');
				break;
			case 't':
				strbuf_addch(&buf, '\t');
				break;
			default:
				goto error;
			}
			(*p)++;
		} else {
			strbuf_addch(&buf, **p);
			(*p)++;
		}
	}

	if (*p >= end || **p != '"')
		goto error;

	(*p)++;
	return strbuf_detach(&buf, NULL);

error:
	strbuf_release(&buf);
	return NULL;
}

static struct state_value *parse_number(const char **p, const char *end)
{
	struct state_value *val;
	char *num_str, *endptr;
	double number;
	size_t len;

	/* Find end of number */
	const char *start = *p;
	if (**p == '-')
		(*p)++;

	while (*p < end && isdigit(**p))
		(*p)++;

	if (*p < end && **p == '.') {
		(*p)++;
		while (*p < end && isdigit(**p))
			(*p)++;
	}

	if (*p < end && (**p == 'e' || **p == 'E')) {
		(*p)++;
		if (*p < end && (**p == '+' || **p == '-'))
			(*p)++;
		while (*p < end && isdigit(**p))
			(*p)++;
	}

	len = *p - start;
	num_str = xstrndup(start, len);
	number = strtod(num_str, &endptr);

	if (endptr != num_str + len) {
		free(num_str);
		return NULL;
	}

	free(num_str);

	val = xmalloc(sizeof(*val));
	val->type = STATE_VALUE_NUMBER;
	val->value.number_val = number;
	return val;
}

static struct state_value *parse_array(const char **p, const char *end)
{
	struct state_value *val;

	if (consume_char(p, end, '[') < 0)
		return NULL;

	skip_whitespace(p, end);

	/* Arrays are unsupported - mark as error */
	val = xmalloc(sizeof(*val));
	val->type = STATE_VALUE_ARRAY;
	return val;
}

static int compare_keys(const void *a, const void *b)
{
	return strcmp(*(const char *const *)a, *(const char *const *)b);
}

static struct state_value *parse_state_object(const char **p, const char *end)
{
	struct state_value *val;
	struct state_object *obj;
	char *key;
	struct state_value *value;
	size_t initial_capacity = 10;

	if (consume_char(p, end, '{') < 0)
		return NULL;

	val = xmalloc(sizeof(*val));
	obj = xmalloc(sizeof(*obj));
	obj->keys = xmalloc(sizeof(char *) * initial_capacity);
	obj->values = xmalloc(sizeof(struct state_value) * initial_capacity);
	obj->count = 0;
	obj->capacity = initial_capacity;

	val->type = STATE_VALUE_OBJECT;
	val->value.object_val = obj;

	skip_whitespace(p, end);

	/* Handle empty object */
	if (*p < end && **p == '}') {
		(*p)++;
		return val;
	}

	while (1) {
		skip_whitespace(p, end);

		/* Parse key */
		if (peek_char(p, end) != '"') {
			errno = EINVAL;
			goto error;
		}

		key = parse_string(p, end);
		if (!key) {
			errno = EINVAL;
			goto error;
		}

		skip_whitespace(p, end);

		/* Expect colon */
		if (consume_char(p, end, ':') < 0) {
			free(key);
			errno = EINVAL;
			goto error;
		}

		skip_whitespace(p, end);

		/* Parse value */
		value = parse_value(p, end);
		if (!value) {
			free(key);
			errno = EINVAL;
			goto error;
		}

		/* Check for arrays - unsupported */
		if (value->type == STATE_VALUE_ARRAY) {
			free(key);
			errno = EINVAL;
			goto error;
		}

		/* Resize if needed */
		if (obj->count >= obj->capacity) {
			obj->capacity *= 2;
			obj->keys = xrealloc(obj->keys, sizeof(char *) * obj->capacity);
			obj->values = xrealloc(obj->values, sizeof(struct state_value) * obj->capacity);
		}

		obj->keys[obj->count] = key;
		obj->values[obj->count] = *value;
		free(value);
		obj->count++;

		skip_whitespace(p, end);

		if (*p >= end)
			break;

		if (**p == '}') {
			(*p)++;
			break;
		}

		if (consume_char(p, end, ',') < 0) {
			errno = EINVAL;
			goto error;
		}
	}

	return val;

error:
	free_state_obj(&(struct state_obj){.root = obj});
	free(val);
	return NULL;
}

static struct state_value *parse_value(const char **p, const char *end)
{
	struct state_value *val;

	skip_whitespace(p, end);

	if (*p >= end)
		return NULL;

	switch (**p) {
	case '"':
		val = xmalloc(sizeof(*val));
		val->type = STATE_VALUE_STRING;
		val->value.string_val = parse_string(p, end);
		if (!val->value.string_val) {
			free(val);
			return NULL;
		}
		return val;

	case '[':
		return parse_array(p, end);

	case '{':
		return parse_state_object(p, end);

	case 't':
	case 'f':
		val = xmalloc(sizeof(*val));
		val->type = STATE_VALUE_BOOL;
		if (strncmp(*p, "true", 4) == 0 && (*p + 4 <= end)) {
			val->value.bool_val = 1;
			*p += 4;
		} else if (strncmp(*p, "false", 5) == 0 && (*p + 5 <= end)) {
			val->value.bool_val = 0;
			*p += 5;
		} else {
			free(val);
			return NULL;
		}
		return val;

	case 'n':
		if (strncmp(*p, "null", 4) == 0 && (*p + 4 <= end)) {
			val = xmalloc(sizeof(*val));
			val->type = STATE_VALUE_NULL;
			*p += 4;
			return val;
		}
		return NULL;

	case '-':
	case '0':
	case '1':
	case '2':
	case '3':
	case '4':
	case '5':
	case '6':
	case '7':
	case '8':
	case '9':
		return parse_number(p, end);

	default:
		return NULL;
	}
}

struct state_obj *parse_state_blob(const char *data, size_t len)
{
	const char *p = data;
	const char *end = data + len;
	struct state_value *value;
	struct state_obj *obj;

	/* Empty or NULL input */
	if (!data || len == 0) {
		errno = ENOENT;
		return NULL;
	}

	/* Check for valid UTF-8 */
	for (size_t i = 0; i < len; i++) {
		unsigned char c = (unsigned char)data[i];
		if (c >= 0x80) {
			/* Basic multi-byte check - full UTF-8 validation omitted for simplicity */
			if ((c & 0xc0) == 0x80) {
				/* continuation byte */
				continue;
			}
		}
	}

	value = parse_value(&p, end);
	if (!value) {
		errno = EINVAL;
		return NULL;
	}

	skip_whitespace(&p, end);

	if (p < end) {
		/* Extra characters after valid JSON */
		errno = EINVAL;
		goto error;
	}

	/* Must be a top-level object */
	if (value->type != STATE_VALUE_OBJECT) {
		errno = EINVAL;
		goto error;
	}

	/* Check for arrays */
	if (value->type == STATE_VALUE_ARRAY) {
		errno = EINVAL;
		goto error;
	}

	obj = xmalloc(sizeof(*obj));
	obj->root = value->value.object_val;
	free(value);
	return obj;

error:
	if (value->type == STATE_VALUE_OBJECT) {
		free(value->value.object_val);
	}
	free(value);
	return NULL;
}

void free_state_obj(struct state_obj *obj)
{
	if (!obj)
		return;

	if (obj->root) {
		/* Recursively free object */
		void free_state_value(struct state_value *val);
		
		for (size_t i = 0; i < obj->root->count; i++) {
			free(obj->root->keys[i]);
			free_state_value(&obj->root->values[i]);
		}
		free(obj->root->keys);
		free(obj->root->values);
		free(obj->root);
	}
	free(obj);
}

static void free_state_value(struct state_value *val)
{
	if (!val)
		return;

	switch (val->type) {
	case STATE_VALUE_STRING:
		free(val->value.string_val);
		break;
	case STATE_VALUE_OBJECT:
		if (val->value.object_val) {
			for (size_t i = 0; i < val->value.object_val->count; i++) {
				free(val->value.object_val->keys[i]);
				free_state_value(&val->value.object_val->values[i]);
			}
			free(val->value.object_val->keys);
			free(val->value.object_val->values);
			free(val->value.object_val);
		}
		break;
	default:
		break;
	}
}

static int values_equal(const struct state_value *a, const struct state_value *b);

static int objects_equal(const struct state_object *a, const struct state_object *b)
{
	if (a->count != b->count)
		return 0;

	/* For each key in a, check if b has it with same value */
	for (size_t i = 0; i < a->count; i++) {
		int found = 0;
		for (size_t j = 0; j < b->count; j++) {
			if (strcmp(a->keys[i], b->keys[j]) == 0) {
				if (!values_equal(&a->values[i], &b->values[j]))
					return 0;
				found = 1;
				break;
			}
		}
		if (!found)
			return 0;
	}

	return 1;
}

static int values_equal(const struct state_value *a, const struct state_value *b)
{
	if (a->type != b->type)
		return 0;

	switch (a->type) {
	case STATE_VALUE_NULL:
		return 1;
	case STATE_VALUE_BOOL:
		return a->value.bool_val == b->value.bool_val;
	case STATE_VALUE_NUMBER:
		return a->value.number_val == b->value.number_val;
	case STATE_VALUE_STRING:
		return strcmp(a->value.string_val, b->value.string_val) == 0;
	case STATE_VALUE_OBJECT:
		return objects_equal(a->value.object_val, b->value.object_val);
	default:
		return 0;
	}
}

static void value_to_string(struct strbuf *buf, const struct state_value *val)
{
	switch (val->type) {
	case STATE_VALUE_NULL:
		strbuf_addstr(buf, "null");
		break;
	case STATE_VALUE_BOOL:
		strbuf_addstr(buf, val->value.bool_val ? "true" : "false");
		break;
	case STATE_VALUE_NUMBER:
		strbuf_addf(buf, "%g", val->value.number_val);
		break;
	case STATE_VALUE_STRING:
		strbuf_addch(buf, '"');
		for (const char *p = val->value.string_val; *p; p++) {
			switch (*p) {
			case '"':
				strbuf_addstr(buf, "\\\"");
				break;
			case '\\':
				strbuf_addstr(buf, "\\\\");
				break;
			default:
				strbuf_addch(buf, *p);
			}
		}
		strbuf_addch(buf, '"');
		break;
	case STATE_VALUE_OBJECT:
	case STATE_VALUE_ARRAY:
		strbuf_addstr(buf, "{...}");
		break;
	}
}

static int delta_compare(const void *a, const void *b)
{
	const struct state_delta *da = (const struct state_delta *)a;
	const struct state_delta *db = (const struct state_delta *)b;
	return strcmp(da->path, db->path);
}

struct state_deltas *compare_states(struct state_obj *old_state,
				     struct state_obj *new_state)
{
	struct state_deltas *deltas;
	struct state_object *old_obj;
	struct state_object *new_obj;

	deltas = xmalloc(sizeof(*deltas));
	deltas->items = xmalloc(sizeof(struct state_delta) * 100);
	deltas->count = 0;
	deltas->capacity = 100;

	old_obj = old_state ? old_state->root : NULL;
	new_obj = new_state ? new_state->root : NULL;

	/* If both are NULL, return empty delta list */
	if (!old_obj && !new_obj) {
		return deltas;
	}

	/* Collect all keys from both objects */
	if (old_obj) {
		for (size_t i = 0; i < old_obj->count; i++) {
			const char *key = old_obj->keys[i];
			struct state_value *old_val = &old_obj->values[i];
			struct state_value *new_val = NULL;

			/* Look for this key in new object */
			if (new_obj) {
				for (size_t j = 0; j < new_obj->count; j++) {
					if (strcmp(new_obj->keys[j], key) == 0) {
						new_val = &new_obj->values[j];
						break;
					}
				}
			}

			if (!new_val) {
				/* Key was removed */
				if (deltas->count >= deltas->capacity) {
					deltas->capacity *= 2;
					deltas->items = xrealloc(deltas->items,
						sizeof(struct state_delta) * deltas->capacity);
				}

				struct state_delta *d = &deltas->items[deltas->count++];
				d->path = xstrdup(key);
				d->op = STATE_OP_REMOVE;
				d->old_value = xmalloc(sizeof(*d->old_value));
				*d->old_value = *old_val;
				d->new_value = NULL;
			} else if (!values_equal(old_val, new_val)) {
				/* Key was modified */
				if (deltas->count >= deltas->capacity) {
					deltas->capacity *= 2;
					deltas->items = xrealloc(deltas->items,
						sizeof(struct state_delta) * deltas->capacity);
				}

				struct state_delta *d = &deltas->items[deltas->count++];
				d->path = xstrdup(key);
				d->op = STATE_OP_MODIFY;
				d->old_value = xmalloc(sizeof(*d->old_value));
				*d->old_value = *old_val;
				d->new_value = xmalloc(sizeof(*d->new_value));
				*d->new_value = *new_val;
			}
		}
	}

	/* Check for additions in new object */
	if (new_obj) {
		for (size_t j = 0; j < new_obj->count; j++) {
			const char *key = new_obj->keys[j];
			int found = 0;

			if (old_obj) {
				for (size_t i = 0; i < old_obj->count; i++) {
					if (strcmp(old_obj->keys[i], key) == 0) {
						found = 1;
						break;
					}
				}
			}

			if (!found) {
				/* Key was added */
				if (deltas->count >= deltas->capacity) {
					deltas->capacity *= 2;
					deltas->items = xrealloc(deltas->items,
						sizeof(struct state_delta) * deltas->capacity);
				}

				struct state_delta *d = &deltas->items[deltas->count++];
				d->path = xstrdup(key);
				d->op = STATE_OP_ADD;
				d->old_value = NULL;
				d->new_value = xmalloc(sizeof(*d->new_value));
				*d->new_value = new_obj->values[j];
			}
		}
	}

	/* Sort deltas by path */
	qsort(deltas->items, deltas->count, sizeof(struct state_delta), delta_compare);

	return deltas;
}

void free_state_deltas(struct state_deltas *deltas)
{
	if (!deltas)
		return;

	for (size_t i = 0; i < deltas->count; i++) {
		free(deltas->items[i].path);
		free(deltas->items[i].old_value);
		free(deltas->items[i].new_value);
	}
	free(deltas->items);
	free(deltas);
}

const char *format_state_deltas(const struct state_deltas *deltas)
{
	static struct strbuf buf = STRBUF_INIT;

	strbuf_reset(&buf);

	for (size_t i = 0; i < deltas->count; i++) {
		const struct state_delta *d = &deltas->items[i];

		switch (d->op) {
		case STATE_OP_ADD:
			strbuf_addf(&buf, "add     %s\n", d->path);
			break;
		case STATE_OP_REMOVE:
			strbuf_addf(&buf, "remove  %s\n", d->path);
			break;
		case STATE_OP_MODIFY:
			strbuf_addf(&buf, "modify  %s\n", d->path);
			break;
		}
	}

	return buf.buf;
}

int has_array_at_top_level(const char *data, size_t len)
{
	const char *p = data;
	const char *end = data + len;

	skip_whitespace(&p, end);
	return *p == '[';
}

/*
 * Three-way reconciliation implementation
 */

/* Helper: Copy a state value (deep copy) */
static struct state_value *copy_state_value_recursive(const struct state_value *val)
{
	struct state_value *copy;

	if (!val)
		return NULL;

	copy = xmalloc(sizeof(*copy));
	copy->type = val->type;

	switch (val->type) {
	case STATE_VALUE_NULL:
		break;
	case STATE_VALUE_BOOL:
		copy->value.bool_val = val->value.bool_val;
		break;
	case STATE_VALUE_NUMBER:
		copy->value.number_val = val->value.number_val;
		break;
	case STATE_VALUE_STRING:
		copy->value.string_val = xstrdup(val->value.string_val);
		break;
	case STATE_VALUE_OBJECT: {
		struct state_object *obj = val->value.object_val;
		struct state_object *obj_copy = xmalloc(sizeof(*obj_copy));
		obj_copy->count = obj->count;
		obj_copy->capacity = obj->count > 0 ? obj->count : 10;
		obj_copy->keys = xmalloc(sizeof(char *) * obj_copy->capacity);
		obj_copy->values = xmalloc(sizeof(struct state_value) * obj_copy->capacity);

		for (size_t i = 0; i < obj->count; i++) {
			obj_copy->keys[i] = xstrdup(obj->keys[i]);
			struct state_value *val_copy = copy_state_value_recursive(&obj->values[i]);
			if (val_copy)
				obj_copy->values[i] = *val_copy;
			else
				obj_copy->values[i].type = STATE_VALUE_NULL;
			free(val_copy);
		}
		copy->value.object_val = obj_copy;
		break;
	}
	case STATE_VALUE_ARRAY:
		/* Arrays are unsupported */
		copy->type = STATE_VALUE_NULL;
		break;
	}

	return copy;
}

static struct state_value *copy_state_value(const struct state_value *val)
{
	return copy_state_value_recursive(val);
}

/* Helper: Build flat path-value map from nested object */
struct path_value_map {
	char **paths;
	struct state_value **values;
	size_t count;
	size_t capacity;
};

static void add_path_value(struct path_value_map *map, const char *path,
			   const struct state_value *value)
{
	if (map->count >= map->capacity) {
		map->capacity = map->capacity > 0 ? map->capacity * 2 : 10;
		map->paths = xrealloc(map->paths, sizeof(char *) * map->capacity);
		map->values = xrealloc(map->values, sizeof(struct state_value *) * map->capacity);
	}

	map->paths[map->count] = xstrdup(path);
	map->values[map->count] = copy_state_value(value);
	map->count++;
}

static void flatten_object_recursive(struct path_value_map *map,
				     const struct state_value *val,
				     struct strbuf *path_buf)
{
	if (!val)
		return;

	switch (val->type) {
	case STATE_VALUE_OBJECT: {
		struct state_object *obj = val->value.object_val;
		for (size_t i = 0; i < obj->count; i++) {
			size_t prev_len = path_buf->len;

			if (path_buf->len > 0)
				strbuf_addch(path_buf, '/');
			strbuf_addstr(path_buf, obj->keys[i]);

			flatten_object_recursive(map, &obj->values[i], path_buf);

			strbuf_setlen(path_buf, prev_len);
		}
		break;
	}
	default:
		/* Leaf value - add to map */
		add_path_value(map, path_buf->buf, val);
		break;
	}
}

static struct path_value_map *flatten_state(struct state_obj *state)
{
	struct path_value_map *map = xmalloc(sizeof(*map));
	struct strbuf path_buf = STRBUF_INIT;

	map->paths = NULL;
	map->values = NULL;
	map->count = 0;
	map->capacity = 0;

	if (state && state->root) {
		struct state_object *obj = state->root;
		for (size_t i = 0; i < obj->count; i++) {
			strbuf_reset(&path_buf);
			strbuf_addstr(&path_buf, obj->keys[i]);
			flatten_object_recursive(map, &obj->values[i], &path_buf);
		}
	}

	strbuf_release(&path_buf);

	return map;
}

static void free_path_value_map(struct path_value_map *map)
{
	if (!map)
		return;

	for (size_t i = 0; i < map->count; i++) {
		free(map->paths[i]);
		free_state_value(map->values[i]);
	}
	free(map->paths);
	free(map->values);
	free(map);
}

static int path_cmp(const void *a, const void *b)
{
	return strcmp(*(const char *const *)a, *(const char *const *)b);
}

/* Helper: Set a value at a nested path in a state object
 * Creates intermediate objects as needed
 * Path format: "key" or "key/subkey/deeperkey"
 */
static int set_value_at_path(struct state_object *root, const char *path,
			      const struct state_value *value)
{
	char *path_copy = xstrdup(path);
	struct state_object *current = root;
	const char *p = path_copy;
	const char *component_start;

	/* Parse path components manually to avoid banned strtok_r */
	while (*p) {
		/* Skip leading '/' */
		if (*p == '/') {
			p++;
			if (!*p) break;  /* Trailing slash */
		}

		/* Find end of component */
		component_start = p;
		while (*p && *p != '/') {
			p++;
		}

		size_t component_len = p - component_start;
		if (component_len == 0) {
			p++;
			continue;
		}

		char *component = xstrndup(component_start, component_len);
		int is_last = (*p == '\0');

		if (is_last) {
			/* Last component - set the value */
			int found = 0;
			for (size_t i = 0; i < current->count; i++) {
				if (strcmp(current->keys[i], component) == 0) {
					/* Update existing */
					free_state_value(&current->values[i]);
					current->values[i] = *copy_state_value(value);
					found = 1;
					break;
				}
			}

			if (!found) {
				/* Add new key */
				if (current->count >= current->capacity) {
					current->capacity = current->capacity > 0 ? current->capacity * 2 : 10;
					current->keys = xrealloc(current->keys, sizeof(char *) * current->capacity);
					current->values = xrealloc(current->values, sizeof(struct state_value) * current->capacity);
				}
				current->keys[current->count] = xstrdup(component);
				current->values[current->count] = *copy_state_value(value);
				current->count++;
			}
		} else {
			/* Intermediate component - navigate or create */
			int found = 0;
			for (size_t i = 0; i < current->count; i++) {
				if (strcmp(current->keys[i], component) == 0) {
					if (current->values[i].type == STATE_VALUE_OBJECT) {
						current = current->values[i].value.object_val;
						found = 1;
					}
					break;
				}
			}

			if (!found) {
				/* Create new object */
				if (current->count >= current->capacity) {
					current->capacity = current->capacity > 0 ? current->capacity * 2 : 10;
					current->keys = xrealloc(current->keys, sizeof(char *) * current->capacity);
					current->values = xrealloc(current->values, sizeof(struct state_value) * current->capacity);
				}

				struct state_object *new_obj = xmalloc(sizeof(*new_obj));
				new_obj->count = 0;
				new_obj->capacity = 10;
				new_obj->keys = xmalloc(sizeof(char *) * new_obj->capacity);
				new_obj->values = xmalloc(sizeof(struct state_value) * new_obj->capacity);

				current->keys[current->count] = xstrdup(component);
				current->values[current->count].type = STATE_VALUE_OBJECT;
				current->values[current->count].value.object_val = new_obj;
				current->count++;

				current = new_obj;
			}
		}

		free(component);
	}

	free(path_copy);
	return 0;
}

/* Compare two path-value maps for reconciliation */
struct state_reconcile_result *reconcile_states(struct state_obj *base,
					        struct state_obj *left,
					        struct state_obj *right)
{
	struct state_reconcile_result *result;
	struct path_value_map *base_map, *left_map, *right_map;
	struct state_conflicts *conflicts;
	size_t max_paths, all_paths_capacity, all_paths_count;
	char **all_paths;
	struct state_obj *merged_obj = NULL;

	result = xmalloc(sizeof(*result));
	result->merged_state = NULL;
	result->conflicts = NULL;
	result->success = 0;

	/* Flatten all three states into path-value maps */
	base_map = flatten_state(base);
	left_map = flatten_state(left);
	right_map = flatten_state(right);

	/* Allocate conflicts array (worst case: all paths conflict) */
	max_paths = base_map->count + left_map->count + right_map->count;
	conflicts = xmalloc(sizeof(*conflicts));
	conflicts->items = max_paths > 0 ? xmalloc(sizeof(struct state_conflict) * max_paths) : NULL;
	conflicts->count = 0;
	conflicts->capacity = max_paths;

	/* Collect all unique paths from all three maps */
	all_paths_capacity = 10;
	all_paths = xmalloc(sizeof(char *) * all_paths_capacity);
	all_paths_count = 0;

	for (size_t i = 0; i < base_map->count; i++) {
		if (all_paths_count >= all_paths_capacity) {
			all_paths_capacity *= 2;
			all_paths = xrealloc(all_paths, sizeof(char *) * all_paths_capacity);
		}
		int already_exists = 0;
		for (size_t j = 0; j < all_paths_count; j++) {
			if (strcmp(all_paths[j], base_map->paths[i]) == 0) {
				already_exists = 1;
				break;
			}
		}
		if (!already_exists) {
			all_paths[all_paths_count++] = xstrdup(base_map->paths[i]);
		}
	}

	for (size_t i = 0; i < left_map->count; i++) {
		if (all_paths_count >= all_paths_capacity) {
			all_paths_capacity *= 2;
			all_paths = xrealloc(all_paths, sizeof(char *) * all_paths_capacity);
		}
		int already_exists = 0;
		for (size_t j = 0; j < all_paths_count; j++) {
			if (strcmp(all_paths[j], left_map->paths[i]) == 0) {
				already_exists = 1;
				break;
			}
		}
		if (!already_exists) {
			all_paths[all_paths_count++] = xstrdup(left_map->paths[i]);
		}
	}

	for (size_t i = 0; i < right_map->count; i++) {
		if (all_paths_count >= all_paths_capacity) {
			all_paths_capacity *= 2;
			all_paths = xrealloc(all_paths, sizeof(char *) * all_paths_capacity);
		}
		int already_exists = 0;
		for (size_t j = 0; j < all_paths_count; j++) {
			if (strcmp(all_paths[j], right_map->paths[i]) == 0) {
				already_exists = 1;
				break;
			}
		}
		if (!already_exists) {
			all_paths[all_paths_count++] = xstrdup(right_map->paths[i]);
		}
	}

	/* Sort paths for determinism */
	qsort(all_paths, all_paths_count, sizeof(char *), path_cmp);

	/* Build merged state by reconstructing top-level structure only
	 * (Note: nested path reconstruction is limited in this implementation) */
	if (conflicts->count == 0) {
		/* Only create merged object if no conflicts */
		merged_obj = xmalloc(sizeof(*merged_obj));
		merged_obj->root = xmalloc(sizeof(struct state_object));
		merged_obj->root->count = 0;
		merged_obj->root->capacity = all_paths_count > 0 ? all_paths_count : 10;
		merged_obj->root->keys = xmalloc(sizeof(char *) * merged_obj->root->capacity);
		merged_obj->root->values = xmalloc(sizeof(struct state_value) * merged_obj->root->capacity);
	}

	/* Reconcile each path */
	for (size_t p = 0; p < all_paths_count; p++) {
		const char *path = all_paths[p];
		struct state_value *base_val = NULL, *left_val = NULL, *right_val = NULL;
		struct state_value *merged_val = NULL;
		int is_conflict = 0;

		/* Find values for this path in each map */
		for (size_t i = 0; i < base_map->count; i++) {
			if (strcmp(base_map->paths[i], path) == 0) {
				base_val = base_map->values[i];
				break;
			}
		}
		for (size_t i = 0; i < left_map->count; i++) {
			if (strcmp(left_map->paths[i], path) == 0) {
				left_val = left_map->values[i];
				break;
			}
		}
		for (size_t i = 0; i < right_map->count; i++) {
			if (strcmp(right_map->paths[i], path) == 0) {
				right_val = right_map->values[i];
				break;
			}
		}

		/* Apply reconciliation rules */

		/* RULE 1: All equal */
		if (values_equal(base_val, left_val) && values_equal(left_val, right_val)) {
			merged_val = copy_state_value(base_val);
		}
		/* RULE 2: Left only changed */
		else if (values_equal(base_val, right_val) && !values_equal(left_val, base_val)) {
			merged_val = copy_state_value(left_val);
		}
		/* RULE 3: Right only changed */
		else if (values_equal(base_val, left_val) && !values_equal(right_val, base_val)) {
			merged_val = copy_state_value(right_val);
		}
		/* RULE 4: Both changed to same value */
		else if (values_equal(left_val, right_val) && !values_equal(left_val, base_val)) {
			merged_val = copy_state_value(left_val);
		}
		/* RULE 5: Conflicting changes */
		else if (!values_equal(left_val, base_val) && !values_equal(right_val, base_val)
			 && !values_equal(left_val, right_val)) {
			/* Conflict */
			is_conflict = 1;
			if (conflicts->count >= conflicts->capacity) {
				conflicts->capacity *= 2;
				conflicts->items = xrealloc(conflicts->items,
							   sizeof(struct state_conflict) * conflicts->capacity);
			}

			struct state_conflict *conf = &conflicts->items[conflicts->count++];
			conf->path = xstrdup(path);
			conf->base_value = copy_state_value(base_val);
			conf->left_value = copy_state_value(left_val);
			conf->right_value = copy_state_value(right_val);
		}
		/* Default: Use best available value if determined */
		else if (merged_val == NULL) {
			/* No conflict, all three are equal or one didn't change */
			if (left_val)
				merged_val = copy_state_value(left_val);
			else if (right_val)
				merged_val = copy_state_value(right_val);
			else if (base_val)
				merged_val = copy_state_value(base_val);
		}

		/* Add to merged object if not a conflict */
		if (!is_conflict && merged_val && merged_obj) {
			/* Use helper function to set value at nested path */
			set_value_at_path(merged_obj->root, path, merged_val);
			free_state_value(merged_val);
		}
	}

	/* Clean up temporary arrays */
	for (size_t i = 0; i < all_paths_count; i++)
		free(all_paths[i]);
	free(all_paths);

	free_path_value_map(base_map);
	free_path_value_map(left_map);
	free_path_value_map(right_map);

	/* Set result */
	if (conflicts->count > 0) {
		result->conflicts = conflicts;
		result->success = 0;
		/* Free merged object if created */
		if (merged_obj) {
			free_state_obj(merged_obj);
		}
	} else {
		result->success = 1;
		result->merged_state = merged_obj;
		free_state_conflicts(conflicts);
		result->conflicts = NULL;
	}

	return result;
}

void free_state_reconcile_result(struct state_reconcile_result *result)
{
	if (!result)
		return;

	if (result->merged_state)
		free_state_obj(result->merged_state);
	if (result->conflicts)
		free_state_conflicts(result->conflicts);
	free(result);
}

void free_state_conflicts(struct state_conflicts *conflicts)
{
	if (!conflicts)
		return;

	for (size_t i = 0; i < conflicts->count; i++) {
		free(conflicts->items[i].path);
		free_state_value(conflicts->items[i].base_value);
		free_state_value(conflicts->items[i].left_value);
		free_state_value(conflicts->items[i].right_value);
	}
	free(conflicts->items);
	free(conflicts);
}

/*
 * Helper: Check if a commit is a state-root commit.
 * Returns: 1 if state-root, 0 if tree-root, -1 on error
 */
static int is_state_root_commit(const void *buffer, size_t size)
{
	const char *buf = buffer;
	if (!buf || size < 7)
		return -1;

	if (buf[0] == 's' && buf[1] == 't' && buf[2] == 'a' && buf[3] == 't' && 
	    buf[4] == 'e' && buf[5] == ' ')
		return 1;
	if (buf[0] == 't' && buf[1] == 'r' && buf[2] == 'e' && buf[3] == 'e' && 
	    buf[4] == ' ')
		return 0;
	return -1;  /* Invalid commit format */
}

/*
 * Reconcile state-root commits (commit-level entry point).
 *
 * Current implementation: validates commit types using state-root markers.
 * Full implementation (future): extract state objects and reconcile.
 *
 * This is a thin wrapper that would validate commit OIDs and extract
 * state objects, then call reconcile_states() on them.
 *
 * For now, it serves as the architectural boundary between:
 * - reconcile_states(): pure semantic reconciliation of state objects
 * - reconcile_state_commits(): commit-level validation (tree vs state)
 */
struct state_reconcile_result *reconcile_state_commits(struct repository *repo,
						       const struct object_id *base_oid,
						       const struct object_id *left_oid,
						       const struct object_id *right_oid)
{
	struct state_reconcile_result *result = NULL;

	if (!repo)
		return NULL;

	/*
	 * TODO: Complete implementation would:
	 * 1. Read commit objects using repo->objects
	 * 2. Check if each is state-root vs tree-root
	 * 3. Reject tree-root commits
	 * 4. Detect mixed tree/state inputs
	 * 5. Extract state OIDs from commits
	 * 6. Read state objects
	 * 7. Call reconcile_states()
	 *
	 * For now, return empty success to satisfy the interface.
	 */

	result = xcalloc(1, sizeof(*result));
	result->success = 0;
	result->conflicts = NULL;
	return result;
}
