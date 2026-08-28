#include "git-compat-util.h"
#include "state-diff.h"
#include <stdio.h>
#include <string.h>

/*
 * Simple test program for state-diff semantic engine
 * 
 * Usage:
 *   git-state-diff-test compare <json_a> <json_b>
 *   git-state-diff-test check-array <json>
 */

int main(int argc, char **argv)
{
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <command> [args]\n", argv[0]);
		fprintf(stderr, "  compare <json_a> <json_b> - Compare two state objects\n");
		fprintf(stderr, "  check-array <json>          - Check if JSON has array at top level\n");
		return 1;
	}

	if (strcmp(argv[1], "compare") == 0 && argc == 4) {
		const char *json_a = argv[2];
		const char *json_b = argv[3];
		size_t len_a = strlen(json_a);
		size_t len_b = strlen(json_b);

		struct state_obj *obj_a = parse_state_blob(json_a, len_a);
		if (!obj_a) {
			fprintf(stderr, "Error parsing state A: %s\n", strerror(errno));
			return 1;
		}

		struct state_obj *obj_b = parse_state_blob(json_b, len_b);
		if (!obj_b) {
			fprintf(stderr, "Error parsing state B: %s\n", strerror(errno));
			free_state_obj(obj_a);
			return 1;
		}

		struct state_deltas *deltas = compare_states(obj_a, obj_b);
		
		printf("%zu deltas:\n", deltas->count);
		for (size_t i = 0; i < deltas->count; i++) {
			const struct state_delta *d = &deltas->items[i];
			const char *op_str = "";
			switch (d->op) {
			case STATE_OP_ADD:
				op_str = "add";
				break;
			case STATE_OP_REMOVE:
				op_str = "remove";
				break;
			case STATE_OP_MODIFY:
				op_str = "modify";
				break;
			}
			printf("  %s %s\n", op_str, d->path);
		}

		free_state_deltas(deltas);
		free_state_obj(obj_a);
		free_state_obj(obj_b);
		return 0;
	}

	if (strcmp(argv[1], "check-array") == 0 && argc == 3) {
		const char *json = argv[2];
		size_t len = strlen(json);
		
		int has_array = has_array_at_top_level(json, len);
		printf("%d\n", has_array);
		return 0;
	}

	fprintf(stderr, "Unknown command: %s\n", argv[1]);
	return 1;
}
