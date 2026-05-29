---
name: finalize-prd
description: Close the loop once all of a PRD's slices are merged — harvest the enriched PRD + the merged code changes, fold durable knowledge into permanent repo docs (docs/design/, docs/impl/), close the PRD tracking issue, then delete the spent PRD. Use when a PRD's work is complete, or the user says "finalize"/"wrap up" a PRD. Provider-aware (gh/fgj).
---

# Finalize PRD

Phase 3: once every slice of a PRD is implemented and merged, migrate the durable knowledge
into the repo's permanent docs and retire the PRD. Invoked as `/finalize-prd <slug | prd-issue#>`.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`**.
Per-provider commands come from `scripts/forge_detect.sh <key>`. The artifact-lifecycle
reference is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Preconditions

Resolve `docs/prd/<slug>/` (from the slug or by mapping the PRD issue # via `prd.md`
`prd_issue:`). Confirm:
- `docs/prd/<slug>/slices/` is **empty** (every slice implemented + its doc GC'd by
  `/implement-issue`);
- the slice issues are **closed** (on GitHub, the PRD issue's sub-issue progress reads
  complete; on Forgejo, every task-list item is ticked).

If anything is outstanding, list it and **stop** — do not finalize partial work.

## Step 2 — Harvest

Read the enriched `docs/prd/<slug>/prd.md` in full, especially `## Implementation notes`.
Then review what actually shipped vs what the PRD proposed:

```bash
git fetch origin
git log --oneline --no-merges origin/main ^<prd-branch-point>   # commits since the PRD started
git diff <prd-branch-point>...origin/main -- <relevant paths>
```

(Use the merged slice PRs / closed issues to bound the range.) Note divergences between the
PRD's intent and the implementation.

## Step 3 — Fold into permanent docs

Match the existing doc voice/structure:
- **Design** — add/update the relevant `docs/design/*` (architecture, plugin/SDK interface)
  and append a dated entry to `docs/design/14-decision-log.md` for any decision made during
  implementation.
- **Milestone** — when the PRD maps to a milestone, add/update `docs/impl/NN-M<NN>-*.md` and
  the `docs/impl/README.md` index/status legend.

Capture *durable* knowledge only — what a future contributor needs — not the slice-by-slice
narrative.

## Step 4 — Close out + delete

- Close the **PRD issue** with a comment linking the doc updates (commit SHA / PR).
  Close form for the detected provider:

  !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_close_issue`

- **Confirm with the user**, then delete the entire `docs/prd/<slug>/` and commit alongside
  the doc updates (the PRD has served its purpose and would only drift from here).

Report: docs touched, PRD issue closed, PRD dir removed.

## Constraints

- **Never finalize partial work** — Step 1 is a hard gate.
- Durable knowledge migrates to `docs/`; transient planning detail is discarded with the PRD.
- Doc edits in **English**, matching the surrounding style.
