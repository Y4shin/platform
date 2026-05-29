# Forge reference (gh / fgj)

Human narrative for the feature-workflow skills. The **executable source of truth** for
per-provider commands is [`scripts/forge_detect.sh`](../../scripts/forge_detect.sh): it
detects the host from `origin` (GitHub → `gh`, Forgejo/Codeberg → `fgj`) and prints the
targeted snippet for a key. Skills inject only the key they need, e.g.

```
!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_pr`
```

Run `scripts/forge_detect.sh keys` for the full list. Keys: `git_type`, `owner`, `repo`,
`auth_check`, `cmd_get_issue`, `cmd_create_issue`, `cmd_list_issues`, `cmd_comment`,
`cmd_close_issue`, `cmd_edit_labels`, `cmd_create_pr`, `ensure_labels`,
`cmd_attach_subissue`, `ownership_note`.

## PRD-issue ↔ slice ownership (provider-specific)

Each PRD has one `prd`-labelled **PRD tracking issue** that owns its slice issues
(`scripts/forge_detect.sh cmd_attach_subissue` emits the wiring for the detected provider).

- **GitHub — native sub-issues (true ownership).** `gh` has no sub-issue subcommand
  (cli/cli#10298), so it's wired via `gh api` (`POST …/issues/<prd#>/sub_issues` with the
  child's **internal id**, not its issue number — passing the number 404s). Real
  parent/child hierarchy + roll-up progress on the PRD issue.
- **Forgejo — no ownership hierarchy (best-effort).** Forgejo/Gitea has only
  depends-on/blocks dependencies, and `fgj` exposes neither sub-issues nor dependencies.
  So model it by convention: a task list of `- [ ] #<child> <title>` on the PRD issue + a
  `Part of #<prd>` line in each slice. ⚠️ This is references + a checklist, **not** enforced
  ownership — don't mistake it for parity with GitHub.
- **Both:** every PR body carries `Closes #<n>` so merging auto-closes its slice; the PRD
  issue's task list is ticked as each lands.

## Label scheme

Created idempotently via `scripts/forge_detect.sh ensure_labels` (emits the provider's
`label create` commands) so a fresh repo self-provisions.

| Label | Meaning |
|-------|---------|
| `kind:feature` / `kind:capability` | track; carried from PRD `kind` onto every slice issue |
| `prd` | the PRD tracking issue (parent of the slices) |
| `mode:hitl` / `mode:afk` | needs human interaction vs autonomously mergeable |
| `status:todo` / `status:in-progress` / `status:needs-review` / `status:done` | slice lifecycle |
| `milestone:M<NN>` *(optional)* | mirror of a provider milestone when one applies |
