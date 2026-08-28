#include "git-compat-util.h"
#include "state-diff.h"
#include <stdio.h>
#include <string.h>

/*
 * Simple test program for state reconciliation engine
 * 
 * Usage:
 *   git-state-reconcile-test reconcile <base_json> <left_json> <right_json>
 *   git-state-reconcile-test check-conflicts <base_json> <left_json> <right_json>
 */

int main(int argc, char **argv)
{
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <command> [args]\n", argv[0]);
		fprintf(stderr, "  reconcile <base> <left> <right>     - Reconcile three states\n");
		fprintf(stderr, "  check-conflicts <base> <left> <right> - Check if reconciliation has conflicts\n");
		return 1;
	}

	if (strcmp(argv[1], "reconcile") == 0 && argc == 5) {
		const char *base_json = argv[2];
		const char *left_json = argv[3];
		const char *right_json = argv[4];
		size_t len_base = strlen(base_json);
		size_t len_left = strlen(left_json);
		size_t len_right = strlen(right_json);

		struct state_obj *base = parse_state_blob(base_json, len_base);
		if (!base && len_base > 0) {
			fprintf(stderr, "Error parsing base state: %s\n", strerror(errno));
			return 1;
		}

		struct state_obj *left = parse_state_blob(left_json, len_left);
		if (!left && len_left > 0) {
			fprintf(stderr, "Error parsing left state: %s\n", strerror(errno));
			free_state_obj(base);
			return 1;
		}

		struct state_obj *right = parse_state_blob(right_json, len_right);
		if (!right && len_right > 0) {
			fprintf(stderr, "Error parsing right state: %s\n", strerror(errno));
			free_state_obj(base);
			free_state_obj(left);
			return 1;
		}

		struct state_reconcile_result *result = reconcile_states(base, left, right);
		
		if (result->success) {
			printf("MERGED\n");
			/* Print merged state (would include the actual merged JSON) */
		} else {
			printf("CONFLICT\n");
			printf("Conflicts: %zu\n", result->conflicts->count);
			for (size_t i = 0; i < result->conflicts->count; i++) {
				struct state_conflict *conf = &result->conflicts->items[i];
				printf("  Path: %s\n", conf->path);
			}
		}

		free_state_reconcile_result(result);
		free_state_obj(base);
		free_state_obj(left);
		free_state_obj(right);
		return 0;
	}

	if (strcmp(argv[1], "check-conflicts") == 0 && argc == 5) {
		const char *base_json = argv[2];
		const char *left_json = argv[3];
		const char *right_json = argv[4];
		size_t len_base = strlen(base_json);
		size_t len_left = strlen(left_json);
		size_t len_right = strlen(right_json);

		struct state_obj *base = parse_state_blob(base_json, len_base);
		if (!base && len_base > 0) {
			fprintf(stderr, "Error parsing base state: %s\n", strerror(errno));
			return 1;
		}

		struct state_obj *left = parse_state_blob(left_json, len_left);
		if (!left && len_left > 0) {
			fprintf(stderr, "Error parsing left state: %s\n", strerror(errno));
			free_state_obj(base);
			return 1;
		}

		struct state_obj *right = parse_state_blob(right_json, len_right);
		if (!right && len_right > 0) {
			fprintf(stderr, "Error parsing right state: %s\n", strerror(errno));
			free_state_obj(base);
			free_state_obj(left);
			return 1;
		}

		struct state_reconcile_result *result = reconcile_states(base, left, right);
		
		if (result->success) {
			printf("0\n");  /* No conflicts */
		} else {
			printf("%zu\n", result->conflicts->count);  /* Number of conflicts */
		}

		free_state_reconcile_result(result);
		free_state_obj(base);
		free_state_obj(left);
		free_state_obj(right);
		return 0;
	}

	fprintf(stderr, "Unknown command: %s\n", argv[1]);
	return 1;
}
