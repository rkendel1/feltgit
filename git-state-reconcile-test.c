#include "git-compat-util.h"
#include "state-diff.h"
#include <stdio.h>
#include <string.h>

/*
 * Test program for state reconciliation engine with JSON output format
 * 
 * Usage:
 *   git-state-reconcile-test reconcile <base_json> <left_json> <right_json>
 *   git-state-reconcile-test check-conflicts <base_json> <left_json> <right_json>
 *   git-state-reconcile-test dump-conflict <base_json> <left_json> <right_json>
 */

static void print_json_string(const char *str)
{
	if (!str) {
		printf("null");
		return;
	}
	printf("\"");
	for (const char *p = str; *p; p++) {
		switch (*p) {
		case '"':
			printf("\\\"");
			break;
		case '\\':
			printf("\\\\");
			break;
		case '\n':
			printf("\\n");
			break;
		case '\r':
			printf("\\r");
			break;
		case '\t':
			printf("\\t");
			break;
		default:
			printf("%c", *p);
		}
	}
	printf("\"");
}

static void print_state_value(struct state_value *val)
{
	if (!val) {
		printf("null");
		return;
	}
	switch (val->type) {
	case STATE_VALUE_NULL:
		printf("null");
		break;
	case STATE_VALUE_BOOL:
		printf("%s", val->value.bool_val ? "true" : "false");
		break;
	case STATE_VALUE_NUMBER:
		printf("%.17g", val->value.number_val);
		break;
	case STATE_VALUE_STRING:
		print_json_string(val->value.string_val);
		break;
	case STATE_VALUE_OBJECT:
		printf("{\"_type\":\"object\"}");
		break;
	case STATE_VALUE_ARRAY:
		printf("{\"_type\":\"array\"}");
		break;
	}
}

int main(int argc, char **argv)
{
	if (argc < 2) {
		fprintf(stderr, "Usage: %s <command> [args]\n", argv[0]);
		fprintf(stderr, "  reconcile <base> <left> <right>      - Reconcile three states (outputs success/conflicts count)\n");
		fprintf(stderr, "  check-conflicts <base> <left> <right> - Check if reconciliation has conflicts\n");
		fprintf(stderr, "  dump-conflict <base> <left> <right>   - Dump detailed conflict information\n");
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
		
		printf("{\"success\":%d,\"conflicts\":%zu}\n", 
		       result->success ? 1 : 0,
		       result->success ? 0 : (result->conflicts ? result->conflicts->count : 0));

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
		
		printf("%d\n", result->success ? 0 : (result->conflicts ? (int)result->conflicts->count : -1));

		free_state_reconcile_result(result);
		free_state_obj(base);
		free_state_obj(left);
		free_state_obj(right);
		return 0;
	}

	if (strcmp(argv[1], "dump-conflict") == 0 && argc == 5) {
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
		
		printf("{\"success\":%d", result->success ? 1 : 0);
		
		if (!result->success && result->conflicts) {
			printf(",\"conflicts\":[");
			for (size_t i = 0; i < result->conflicts->count; i++) {
				struct state_conflict *conf = &result->conflicts->items[i];
				if (i > 0) printf(",");
				printf("{\"path\":");
				print_json_string(conf->path);
				printf(",\"base\":");
				print_state_value(conf->base_value);
				printf(",\"left\":");
				print_state_value(conf->left_value);
				printf(",\"right\":");
				print_state_value(conf->right_value);
				printf("}");
			}
			printf("]");
		}
		printf("}\n");

		free_state_reconcile_result(result);
		free_state_obj(base);
		free_state_obj(left);
		free_state_obj(right);
		return 0;
	}

	if (strcmp(argv[1], "validate-commit") == 0 && argc == 3) {
		/* validate-commit <oid>: Validate a commit is a state-root commit */
		/* This validates the repository isolation property */
		printf("NOT-IMPLEMENTED\n");
		return 0;
	}

	fprintf(stderr, "Unknown command: %s\n", argv[1]);
	return 1;
}
