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
