---
name: finalize-epic
description: Close the loop on an epic once all its child PRDs are finalized — fold epic-level durable knowledge into permanent repo docs (docs/design/, docs/impl/), close the epic tracking issue, then delete the spent epic dir. Use when every child PRD of an epic is done, or the user says "finalize"/"wrap up" an epic. Provider-aware (gh/fgj).
---

# Finalize Epic

The tier above `/finalize-prd`: once **every** child PRD of an epic has been finalized,
migrate the epic-level durable knowledge into the repo's permanent docs and retire the epic.
Invoked as `/finalize-epic <slug | epic-issue#>`.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`**.
Per-provider commands come from `scripts/forge_detect.sh <key>`. The artifact-lifecycle
reference is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Preconditions (hard gate)

Resolve `docs/prd/epics/<slug>/epic.md` (from the slug or by mapping the epic issue # via
`epic_issue:`). Confirm **every** child in `prds:` is finalized:
- each child's `docs/prd/<child-slug>/` directory is **gone** (deleted by `/finalize-prd`);
- each child PRD issue is **closed** (the epic issue's sub-issue progress reads complete).

If any child is outstanding, list it and **stop** — finalize the remaining child PRDs first
(`/finalize-prd <child-slug>`). Never finalize a partial epic.

## Step 2 — Harvest

Read `epic.md` in full. The per-PRD durable knowledge already landed in `docs/design/` +
`docs/impl/` via each `/finalize-prd`; your job here is the **cross-cutting** story the
individual PRDs couldn't tell on their own:
- how the plugins compose to deliver the outcome (the seams, the shared tables/components,
  the navigation/permission wiring);
- any epic-level decision not captured by a single child.

## Step 3 — Fold into permanent docs

Match the existing doc voice/structure:
- **Design** — add/update the relevant `docs/design/*` (especially
  `08-cross-plugin-composition.md` for how the set fits together) and append a dated entry to
  `docs/design/14-decision-log.md` for any epic-level decision.
- **Milestone** — when the epic maps to a milestone (or a band of them), add/update the
  `docs/impl/NN-M<NN>-*.md` doc(s) and the `docs/impl/README.md` index.

Capture *durable, cross-cutting* knowledge only — not what already lives in the child PRDs'
finalized docs.

## Step 4 — Close out + delete

- Close the **epic issue** with a comment linking the doc updates (commit SHA / PR).
  Close form for the detected provider:

  !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_close_issue`

- **Confirm with the user**, then delete the entire `docs/prd/epics/<epic-slug>/` and commit
  alongside the doc updates.

Report: docs touched, epic issue closed, epic dir removed.

## Constraints

- **Never finalize a partial epic** — Step 1 is a hard gate on all children.
- Durable, cross-cutting knowledge migrates to `docs/`; transient planning detail is discarded
  with the epic dir.
- Doc edits in **English**, matching the surrounding style.
