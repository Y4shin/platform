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
`cmd_attach_subissue`, `cmd_detach_subissue`, `cmd_add_dependency`, `ownership_note`.

## Tracker shape — flat, native primitives

The tracker uses GitHub's **native sub-issues** and **native issue dependencies**. One rule
splits the two mechanisms:

- **Sub-issue (parent/child)** — used for **exactly one** relationship: an **epic** is the
  sub-issue parent of its child PRD issues *and* their slice issues, all flat siblings under
  the epic. `scripts/forge_detect.sh cmd_attach_subissue` emits the wiring;
  `cmd_detach_subissue` removes it (the migration uses it to re-home old PRD children).
- **Dependency (`blocked_by`)** — used for **everything else**: a PRD issue is `blocked_by` its
  slice issues; slice `blocked_by` slice for ordering; PRD `blocked_by` PRD for cross-PRD
  order. `scripts/forge_detect.sh cmd_add_dependency` emits the wiring.

A **standalone PRD** (no epic) uses the same rule with the parenting half empty: no sub-issue
parent; the PRD issue is `blocked_by` its slices; slices ordered by dependencies.

### Provider notes

- **GitHub.** `gh` has no sub-issue/dependency subcommand (cli/cli#10298), so both go via
  `gh api`. Sub-issues: `POST …/issues/<epic#>/sub_issues` with the child's **internal id**
  (not its number — passing the number 404s). Dependencies: `POST
  …/issues/<issue#>/dependencies/blocked_by` with `issue_id` = the blocker's **internal id**,
  header `X-GitHub-Api-Version: 2026-03-10`. Both GA as of 2025.
- **Forgejo/Gitea.** Native issue **dependencies** (depends-on/blocks) exist via the API;
  native **sub-issues** vary by version — if unavailable, emulate epic→child by convention
  (epic task list + `Part of #<epic>` line). Confirm against your `fgj` build.
- **Both:** every PR body carries `Closes #<n>` so merging auto-closes its slice.

## Label scheme

Created idempotently via `scripts/forge_detect.sh ensure_labels` (emits the provider's
`label create` commands) so a fresh repo self-provisions.

| Label | Meaning |
|-------|---------|
| `epic` | the epic issue (sub-issue parent of its child PRD + slice issues) |
| `kind:feature` / `kind:capability` | track; carried from PRD `kind` onto every slice issue |
| `prd` | the PRD issue (a regular issue, `blocked_by` its slices — no longer their sub-issue parent) |
| `mode:hitl` / `mode:afk` | needs human interaction vs autonomously mergeable |
| `status:todo` / `status:in-progress` / `status:needs-review` / `status:done` | slice lifecycle |
| `milestone:M<NN>` *(optional)* | mirror of a provider milestone when one applies |
