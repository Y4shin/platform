---
name: epic-to-prds
description: Decompose an epic (kind:epic) into an ordered set of child PRDs, create the epic tracking issue (label epic), and hand off to /create-feature-prd or /create-capability-prd per child with seeded context. Use after /create-epic. Provider-aware (gh/fgj).
---

# Epic → PRDs

Convert a `kind: epic` artifact into an **ordered decomposition plan** of child PRDs and an
**epic issue** that will own them. This skill plans + hands off — it does **not** write the
child PRD bodies (each child gets its own deep `/create-*-prd` grilling so the specs stay
sharp).

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`** — !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ownership_note`

Per-provider commands come from `scripts/forge_detect.sh <key>`, injected at the step that uses
them. The PRD/artifact reference (three tiers + frontmatter + tracker shape + lifecycle) is
injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 0 — Provider + epic

Verify auth, then locate the epic at `docs/prd/epics/<slug>/epic.md` and **assert `kind: epic`** — if
it's `feature`/`capability`, stop and point at `/feature-prd-to-issues` /
`/capability-prd-to-issues`.

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" auth_check`

**Receipt guard:** check `epic.md` `receipts:` for a prior `epic-to-prds` entry (see **Run
receipts** in the injected reference). If present (or `status:` is already `prds-planned`/later),
the epic was already decomposed — report "`epic-to-prds` already ran on `<date>`" and confirm an
intentional re-run before re-creating the epic issue.

Ensure the label scheme exists (idempotent — provisions the `epic` label too):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ensure_labels`

## Step 1 — Draft the child-PRD decomposition

Break the epic into the **fewest coherent PRDs** that each stand alone as a feature or a
capability:

<decomposition-rules>
- Each child is exactly one PRD: a `kind: feature` (one plugin's user-facing behaviour) or a
  `kind: capability` (one foundational SDK/macro/host unit, named with its first consumer).
- Put the **shared / foundational work** into capability PRD(s) that land **before** the
  feature PRDs that consume them.
- Give each child a slug, a one-line scope, and its `blocked_by` (other child slugs).
- Prefer a small number of substantial PRDs — the slices inside each are where granularity
  lives, not here.
</decomposition-rules>

## Step 2 — Quiz the user

Present the decomposition as a numbered list; per child: **slug**, **kind (feature/capability)**,
**one-line scope**, **blocked-by**. Ask: right split into feature vs capability? foundational
work correctly front-loaded? dependency order correct? merge/split any? Iterate until approved.

## Step 3 — Create the epic issue + record the plan

Create-issue form for the detected provider:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_issue`

1. **Create the epic issue** (label `epic`): body = the outcome summary + a checklist of the
   planned child PRDs (`- [ ] <slug> (<kind>)`). Record its number as `epic_issue:` in
   `epic.md`.
2. Fill `prds:` in `epic.md` with the ordered plan (`slug`, `kind`, `issue: null`, `blocked_by`)
   and write the `## Decomposition` section (the same list, with rationale).
3. Set `status: prds-planned` and add an `epic-to-prds` entry to `epic.md` `receipts:`
   (`on: <today>`, `note:` the epic issue #) per **Run receipts**. Commit the
   `docs/prd/epics/<slug>/` changes.

The child PRD issues do **not** exist yet — they're created by the per-child `/create-*-prd` →
`/*-prd-to-issues` runs, which attach themselves under this epic (see those skills).

## Step 4 — Hand off (dependency order, unblocked children first)

For each child, in dependency order, print the command to run, seeding the epic context:

- capability → `/create-capability-prd` for `<scope>` — set `epic: <epic-slug>` in its
  frontmatter.
- feature → `/create-feature-prd` for `<scope>` — set `epic: <epic-slug>` in its frontmatter.

Tell the user to start with the unblocked children. As each child finishes `/*-prd-to-issues`,
its PRD issue + slices attach under the epic and its `prds[].issue` is filled.

After publishing, report: the epic issue number, and `<slug> · feature|capability · blocked-by:
…` per child.

## Constraints

- **kind:epic only** — abort on a feature/capability PRD.
- **English**; **no speculative scope** — every child PRD earns its place in the outcome.
- Plan + hand off only — do not write child PRD bodies or create slice issues here.
- Do not modify unrelated issues.
