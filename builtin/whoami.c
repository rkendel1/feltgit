#include "builtin.h"
#include "config.h"
#include "gettext.h"
#include "gpg-interface.h"
#include "ident.h"
#include "parse-options.h"
#include "strbuf.h"

static const char * const whoami_usage[] = {
	N_("git whoami [options]"),
	NULL
};

struct whoami_config {
	int gpgsign;
	char *signing_key;
	char *gpg_format;
	char *ssh_default_key_cmd;
};

static int whoami_config_cb(const char *var, const char *value,
			    const struct config_context *ctx, void *data)
{
	struct whoami_config *cfg = data;

	if (!strcmp(var, "commit.gpgsign")) {
		cfg->gpgsign = git_config_bool(var, value);
		return 0;
	}
	if (!strcmp(var, "user.signingkey"))
		return git_config_string(&cfg->signing_key, var, value);
	if (!strcmp(var, "gpg.format"))
		return git_config_string(&cfg->gpg_format, var, value);
	if (!strcmp(var, "gpg.ssh.defaultkeycommand"))
		return git_config_string(&cfg->ssh_default_key_cmd, var, value);

	return git_default_config(var, value, ctx, data);
}

int cmd_whoami(int argc,
	       const char **argv,
	       const char *prefix,
	       struct repository *repo)
{
	int show_author = 0;
	int show_committer = 0;
	int show_name = 0;
	int show_email = 0;
	int show_signing_key = 0;
	int porcelain = 0;
	int nul_term = 0;
	int verbose = 0;
	int ret = 0;
	char eol;

	struct option whoami_options[] = {
		OPT_BOOL('a', "author", &show_author, N_("show author identity")),
		OPT_BOOL('c', "committer", &show_committer, N_("show committer identity")),
		OPT_BOOL('n', "name", &show_name, N_("show name only")),
		OPT_BOOL('e', "email", &show_email, N_("show email only")),
		OPT_BOOL('s', "signing-key", &show_signing_key, N_("show commit signing key")),
		OPT_BOOL(0, "porcelain", &porcelain, N_("machine-readable output")),
		OPT_BOOL('z', "null", &nul_term, N_("terminate entries with NUL")),
		OPT__VERBOSE(&verbose, N_("show detailed identity and signing status")),
		OPT_END()
	};

	struct whoami_config cfg = { 0 };
	struct strbuf author_info = STRBUF_INIT;
	struct strbuf committer_info = STRBUF_INIT;
	struct ident_split author_split, committer_split;
	struct strbuf author_name = STRBUF_INIT;
	struct strbuf author_email = STRBUF_INIT;
	struct strbuf committer_name = STRBUF_INIT;
	struct strbuf committer_email = STRBUF_INIT;
	char *resolved_key = NULL;
	int is_ssh = 0;

	argc = parse_options(argc, argv, prefix, whoami_options,
			     whoami_usage, 0);

	if (argc > 0)
		usage_with_options(whoami_usage, whoami_options);

	die_for_incompatible_opt2(show_author, "--author", show_committer, "--committer");
	die_for_incompatible_opt2(show_name, "--name", show_email, "--email");
	die_for_incompatible_opt2(show_signing_key, "--signing-key", show_name, "--name");
	die_for_incompatible_opt2(show_signing_key, "--signing-key", show_email, "--email");
	die_for_incompatible_opt2(show_signing_key, "--signing-key", show_author, "--author");
	die_for_incompatible_opt2(show_signing_key, "--signing-key", show_committer, "--committer");
	die_for_incompatible_opt2(show_signing_key, "--signing-key", verbose, "--verbose");
	die_for_incompatible_opt2(porcelain, "--porcelain", show_author, "--author");
	die_for_incompatible_opt2(porcelain, "--porcelain", show_committer, "--committer");
	die_for_incompatible_opt2(porcelain, "--porcelain", show_name, "--name");
	die_for_incompatible_opt2(porcelain, "--porcelain", show_email, "--email");
	die_for_incompatible_opt2(porcelain, "--porcelain", show_signing_key, "--signing-key");
	die_for_incompatible_opt2(porcelain, "--porcelain", verbose, "--verbose");
	die_for_incompatible_opt2(verbose, "--verbose", show_name, "--name");
	die_for_incompatible_opt2(verbose, "--verbose", show_email, "--email");
	die_for_incompatible_opt2(verbose, "--verbose", show_author, "--author");
	die_for_incompatible_opt2(verbose, "--verbose", show_committer, "--committer");
	die_for_incompatible_opt2(verbose, "--verbose", nul_term, "-z/--null");

	eol = nul_term ? '\0' : '\n';

	repo_config(repo, whoami_config_cb, &cfg);

	strbuf_addstr(&author_info, git_author_info(IDENT_NO_DATE));
	strbuf_addstr(&committer_info, git_committer_info(IDENT_NO_DATE));

	if (split_ident_line(&author_split, author_info.buf, author_info.len) == 0) {
		if (author_split.name_begin && author_split.name_end)
			strbuf_add(&author_name, author_split.name_begin,
				   author_split.name_end - author_split.name_begin);
		if (author_split.mail_begin && author_split.mail_end)
			strbuf_add(&author_email, author_split.mail_begin,
				   author_split.mail_end - author_split.mail_begin);
	}

	if (split_ident_line(&committer_split, committer_info.buf, committer_info.len) == 0) {
		if (committer_split.name_begin && committer_split.name_end)
			strbuf_add(&committer_name, committer_split.name_begin,
				   committer_split.name_end - committer_split.name_begin);
		if (committer_split.mail_begin && committer_split.mail_end)
			strbuf_add(&committer_email, committer_split.mail_begin,
				   committer_split.mail_end - committer_split.mail_begin);
	}

	is_ssh = cfg.gpg_format && !strcmp(cfg.gpg_format, "ssh");

	if (cfg.signing_key && *cfg.signing_key) {
		resolved_key = xstrdup(cfg.signing_key);
	} else if (is_ssh) {
		if (cfg.ssh_default_key_cmd && *cfg.ssh_default_key_cmd)
			resolved_key = get_signing_key_id();
	} else if (cfg.gpgsign) {
		resolved_key = get_signing_key_id();
	}

	if (show_signing_key) {
		if (resolved_key && *resolved_key) {
			printf("%s%c", resolved_key, eol);
			ret = 0;
		} else {
			ret = 1;
		}
		goto cleanup;
	}

	if (show_name) {
		if (show_author)
			printf("%s%c", author_name.buf, eol);
		else
			printf("%s%c", committer_name.buf, eol);
		goto cleanup;
	}

	if (show_email) {
		if (show_author)
			printf("%s%c", author_email.buf, eol);
		else
			printf("%s%c", committer_email.buf, eol);
		goto cleanup;
	}

	if (show_author) {
		printf("%s%c", author_info.buf, eol);
		goto cleanup;
	}

	if (show_committer) {
		printf("%s%c", committer_info.buf, eol);
		goto cleanup;
	}

	if (porcelain || (nul_term && !verbose)) {
		printf("user.author.name=%s%c", author_name.buf, eol);
		printf("user.author.email=%s%c", author_email.buf, eol);
		printf("user.committer.name=%s%c", committer_name.buf, eol);
		printf("user.committer.email=%s%c", committer_email.buf, eol);
		printf("user.signingkey=%s%c",
		       (cfg.signing_key && *cfg.signing_key) ? cfg.signing_key :
		       (resolved_key && *resolved_key) ? resolved_key : "none",
		       eol);
		printf("gpg.format=%s%c",
		       cfg.gpg_format ? cfg.gpg_format : "openpgp", eol);
		printf("commit.gpgsign=%s%c",
		       cfg.gpgsign ? "true" : "false", eol);
		goto cleanup;
	}

	if (verbose) {
		printf(_("Author Name:      %s\n"), author_name.buf);
		printf(_("Author Email:     %s\n"), author_email.buf);
		printf(_("Committer Name:   %s\n"), committer_name.buf);
		printf(_("Committer Email:  %s\n"), committer_email.buf);
		if (cfg.signing_key && *cfg.signing_key)
			printf(_("Signing Key:      %s\n"), cfg.signing_key);
		else if (resolved_key && *resolved_key)
			printf(_("Signing Key:      %s (default fallback)\n"), resolved_key);
		else
			printf(_("Signing Key:      %s\n"), _("none"));
		printf(_("Signing Format:   %s\n"),
		       cfg.gpg_format ? cfg.gpg_format : "openpgp");
		printf(_("GPG Signing:      %s\n"),
		       cfg.gpgsign ? _("enabled") : _("disabled"));
	} else {
		printf(_("Author:    %s\n"), author_info.buf);
		printf(_("Committer: %s\n"), committer_info.buf);
		if (cfg.gpgsign) {
			if (cfg.signing_key && *cfg.signing_key) {
				printf(_("Signing:   %s (format: %s, commit.gpgsign: true)\n"),
				       cfg.signing_key,
				       cfg.gpg_format ? cfg.gpg_format : "openpgp");
			} else if (resolved_key && *resolved_key) {
				printf(_("Signing:   default key (%s) (format: %s, commit.gpgsign: true)\n"),
				       resolved_key,
				       cfg.gpg_format ? cfg.gpg_format : "openpgp");
			} else {
				printf(_("Signing:   enabled (no signing key configured)\n"));
			}
		} else {
			if (cfg.signing_key && *cfg.signing_key) {
				printf(_("Signing:   disabled (key: %s, format: %s, commit.gpgsign: false)\n"),
				       cfg.signing_key,
				       cfg.gpg_format ? cfg.gpg_format : "openpgp");
			} else {
				printf(_("Signing:   disabled (commit.gpgsign: false)\n"));
			}
		}
	}

cleanup:
	free(cfg.signing_key);
	free(cfg.gpg_format);
	free(cfg.ssh_default_key_cmd);
	free(resolved_key);
	strbuf_release(&author_info);
	strbuf_release(&committer_info);
	strbuf_release(&author_name);
	strbuf_release(&author_email);
	strbuf_release(&committer_name);
	strbuf_release(&committer_email);

	return ret;
}
