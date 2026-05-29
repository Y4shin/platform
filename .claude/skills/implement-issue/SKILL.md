---
name: implement-issue
description: Phase 2 — implement a slice issue via strict TDD against the test plan from /analyse-issue. Creates the branch, runs red→green→refactor, opens a PR (Closes #n), sets issue state, then garbage-collects the slice doc and notes the decision on the PRD. Use after /analyse-issue, or when the user says "now implement #n". Provider-aware (gh/fgj).
---

# Implement Issue

Phase 2: execute the agreed test plan with full repo automation. Requires the slice doc
`docs/prd/<slug>/slices/<n>-<slug>.md` (spec + `## Test plan`) written by `/analyse-issue`.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`**.
Per-provider commands come from `scripts/forge_detect.sh <key>`, injected at the step that
uses them. The artifact-lifecycle reference is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Set state

Swap the issue's label `status:todo` → `status:in-progress` and add a starting comment
("Starting implementation. Branch: `feature/<n>-<slug>`."). Label-edit + comment form:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_edit_labels`
!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_comment`

## Step 2 — Sync + branch

```bash
git fetch origin
git checkout main && git pull --ff-only origin main
git checkout -b feature/<n>-<slug>
```

Slug = issue title, 3–5 words, lowercase, hyphens.

## Step 3 — Load context

Read the committed slice doc `docs/prd/<slug>/slices/<n>-<slug>.md` (spec + test plan) **and**
its parent `docs/prd/<slug>/prd.md` — this is the in-repo context the canonical dir exists
to provide. Re-read repo conventions if not in context: `clippy.toml`, `docs/design/`,
`docs/plugin-authoring-guide.md`.

**Repo non-negotiables:**
- No `unsafe` (`unsafe_code = "forbid"`).
- No clippy-disallowed methods (direct env reads, etc. — see `clippy.toml`); go through
  `junius-sdk`.
- English comments, only where the *why* is non-obvious.
- Plugin work treats `plugin.toml` as the source of truth.

## Step 4 — TDD: red → green → refactor

The slice doc's `## Test plan` is the source of truth for test type, file path, run
command, and assertions. Never skip or merge phases.

```
TDD Progress:
- [ ] RED:      test written, confirmed failing
- [ ] GREEN:    minimum implementation, test passing
- [ ] REFACTOR: cleaned up, suite still green
```

- **RED** — write the test first; derive every assertion from the spec/acceptance criteria,
  never from the implementation. Run it; it **must fail**. (A test passing before code
  exists is wrong — rewrite it.) File + command per the test plan:
  `*.rs` in `tests/` or `#[cfg(test)]` → `task test:rust` / `task test:rust:unit`;
  `*.test.tsx` → `task test:js`; `*.spec.ts` E2E → `task test:e2e`.
- **GREEN** — write only what makes the failing test pass; apply Step 3 conventions; re-run
  after each logical change.
- **REFACTOR** — remove dead code, rename for clarity; no speculative abstractions; re-run.

## Step 5 — Acceptance checklist

```
- [ ] Test written before implementation, confirmed failing first (RED)
- [ ] Every acceptance criterion has an assertion
- [ ] Identifier types match the spec (UUID vs numeric — never swap)
- [ ] Error variants / status codes have separate cases
- [ ] task ci passes (fmt, clippy -D warnings, tests, biome, buf) with zero skips
- [ ] If SQL changed: cargo sqlx prepare --workspace refreshed (.sqlx); task sqlx:check passes
- [ ] No unsafe; no clippy-disallowed methods; comments English + only where non-obvious
```

## Step 6 — PR + state

```bash
git push -u origin feature/<n>-<slug>
```

Open a PR with `--base main`, title `<issue title>`, body listing the met acceptance
criteria and `Closes #<n>` (PR form for the detected provider):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_pr`

Then set the issue label `status:in-progress` → `status:needs-review` and tick the slice
off the PRD issue's task list (`- [x] #<n> …`). Label-edit form:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_edit_labels`

Report the PR URL.

## Step 7 — Artifact GC + PRD note

1. Append a 2–4 line decision/deviation note to the PRD's `## Implementation notes` in
   `docs/prd/<slug>/prd.md` (what shipped, any divergence from the spec, follow-ups) — this
   is what `/finalize-prd` harvests.
2. **Delete the slice doc** `docs/prd/<slug>/slices/<n>-<slug>.md` and commit (with the PRD
   note). A surviving slice doc now reliably signals unfinished work.

## Constraints

- **Spec-first** — never write a test to match a wrong implementation.
- **No speculative code** — implement only what the slice requires.
- Issue/PR text in **English**.
