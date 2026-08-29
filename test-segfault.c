#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef enum {
	STATE_VALUE_NULL,
	STATE_VALUE_BOOL,
	STATE_VALUE_NUMBER,
	STATE_VALUE_STRING,
	STATE_VALUE_OBJECT,
	STATE_VALUE_ARRAY,
} state_value_type;

struct state_value;

struct state_object {
	char **keys;
	struct state_value *values;
	size_t count;
	size_t capacity;
};

struct state_value {
	state_value_type type;
	union {
		int bool_val;
		double number_val;
		char *string_val;
		struct state_object *object_val;
	} value;
};

int main() {
	// Test the segfault scenario
	struct state_object *root = malloc(sizeof(*root));
	root->count = 0;
	root->capacity = 2;
	root->keys = calloc(2, sizeof(char *));
	root->values = calloc(2, sizeof(struct state_value));
	
	printf("Root initialized: count=%zu, capacity=%zu\n", root->count, root->capacity);
	printf("Values array address: %p\n", (void*)root->values);
	printf("Values[0]: type=%d\n", root->values[0].type);
	
	// Try to add a key
	root->keys[0] = strdup("a");
	root->values[0].type = STATE_VALUE_NUMBER;
	root->values[0].value.number_val = 1.0;
	root->count++;
	
	printf("After adding key 0: count=%zu\n", root->count);
	
	// Try to add another key
	root->keys[1] = strdup("b");
	root->values[1].type = STATE_VALUE_NUMBER;
	root->values[1].value.number_val = 2.0;
	root->count++;
	
	printf("After adding key 1: count=%zu\n", root->count);
	printf("Success!\n");
	
	return 0;
}
