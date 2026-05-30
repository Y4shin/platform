# PRD + work-artifact reference

Shared by the `create-epic`, `epic-to-prds`, `create-*-prd`, `*-prd-to-issues`,
`analyse-issue`, `implement-issue`, `finalize-prd`, and `finalize-epic` skills. Defines the
three planning tiers, the committed work directory, the tracker shape, and the lifecycle that
keeps it self-cleaning.

## Three tiers

- **epic** (`kind: epic`) — a coordinated outcome that spans several PRDs ("a set of plugins
  that do X"). Owns child PRDs; has no slices of its own. *Optional* — a lone PRD needs no epic.
- **PRD** (`kind: feature | capability`) — one plugin feature, or one foundational capability.
  Broken into slices. May belong to an epic (`epic:` field) or stand alone.
- **slice** — one independently-grabbable issue: a vertical tracer-bullet (feature) or an
  enabling unit + first consumer (capability).

## Canonical work directory (committed)

All planning artifacts live under `docs/prd/<slug>/`, version-controlled so any agent picking
up an issue reads the full spec straight from the repo:

```
docs/prd/epics/<epic-slug>/
  epic.md                # kind: epic — the epic brief + decomposition; NO slices/ subdir

docs/prd/<prd-slug>/
  prd.md                 # the PRD (frontmatter below) + an "## Implementation notes" log
  slices/
    <n>-<slug>.md        # one per slice/issue: spec + (after analyse-issue) "## Test plan"
```

Epics live in their own `docs/prd/epics/` namespace; PRDs sit directly under `docs/prd/`. They
are linked by the PRD's `epic:` field, **not** by nesting — the epic outlives its children
(each child dir self-deletes at `finalize-prd`, but the epic dir survives until `finalize-epic`,
so keeping their directories independent keeps the self-cleaning rules unambiguous). `<slug>` is
the artifact's `slug:`. `<n>` is the slice's issue number once it exists.

## Frontmatter

### Epic (`epic.md`)

```yaml
---
kind: epic
title: <short human title>
slug: <kebab-slug>
epic_issue: <#n>           # filled by epic-to-prds: the epic issue (label: epic)
prds:                      # filled by epic-to-prds: the ordered decomposition
  - slug: <prd-slug>
    kind: feature | capability
    issue: <#n>            # the child PRD's issue once created (null until then)
    blocked_by: [<prd-slug>, ...]
status: draft              # draft | prds-planned | in-progress | done
---
```

### PRD (`prd.md`)

```yaml
---
kind: feature        # feature | capability — drives which prd-to-issues variant consumes it
title: <short human title>
slug: <kebab-slug>   # dir name + branch/issue slugs
epic: <epic-slug>    # OPTIONAL — present when this PRD belongs to an epic; omit if standalone
milestone: M<NN>     # optional; links to a docs/impl/ milestone
prd_issue: <#n>      # filled by *-prd-to-issues: this PRD's own issue (label: prd)
slices: [<#a>, <#b>] # filled by *-prd-to-issues: child (slice) issue numbers
status: draft        # draft | issues-created | in-progress | done
---
```

`feature-prd-to-issues` asserts `kind: feature`; `capability-prd-to-issues` asserts
`kind: capability`; `epic-to-prds` asserts `kind: epic`. Each refuses a mismatched artifact and
points at the correct skill.

## Tracker shape (flat, native primitives)

The tracker uses GitHub's **native sub-issues** and **native issue dependencies** (see
[`forge.md`](forge.md) for the exact commands). One rule splits the two mechanisms:

- **Sub-issue (parent/child)** is used for **exactly one** relationship: an **epic** is the
  sub-issue parent of its child PRD issues *and* their slice issues — all flat siblings under
  the epic. The epic is the only parent.
- **Dependency (`blocked_by`)** expresses **everything else**: a PRD issue is `blocked_by` its
  slice issues (it can't close until its slices land); slice `blocked_by` slice for ordering;
  PRD `blocked_by` PRD for cross-PRD order within an epic.

A **standalone PRD** (no `epic:`) uses the same rule with the parenting half empty: no
sub-issue parent; the PRD issue is `blocked_by` its slices; slices ordered by dependencies.

PRD↔slice membership is recoverable from the committed `slices/<n>-<slug>.md` docs and from the
`PRD blocked_by slice` dependency edges.

## Lifecycle / garbage collection

Artifacts are **deleted as their work lands**, so the presence of a file is itself state:

1. *(optional)* `create-epic` → writes `epic.md` (status `draft`).
2. *(optional)* `epic-to-prds` → creates the epic issue, writes the ordered `prds:` plan +
   `## Decomposition`, status `prds-planned`. Hands off to the per-child `create-*-prd`.
3. `create-(feature|capability)-prd` → writes `prd.md` (status `draft`); carries `epic:` when
   created in an epic context.
4. `(feature|capability)-prd-to-issues` → creates the PRD issue + slice issues, adds the native
   dependencies (and, under an epic, the sub-issue parenting), writes one
   `slices/<n>-<slug>.md` per slice, fills `prd_issue:` + `slices:`, status `issues-created`.
5. `analyse-issue <n>` → appends a `## Test plan` section to that slice's doc.
6. `implement-issue <n>` → on completion appends a note to `prd.md` `## Implementation notes`,
   then **deletes `slices/<n>-<slug>.md`**. A surviving slice doc ⇒ unfinished work.
7. `finalize-prd <slug>` → once `slices/` is empty, migrates durable knowledge into
   `docs/design/` + `docs/impl/`, ticks the epic's `prds[]` entry if any, closes the PRD issue,
   then **deletes `docs/prd/<prd-slug>/`**.
8. *(optional)* `finalize-epic <slug>` → once every child PRD is finalized, migrates
   epic-level knowledge into `docs/design/` + `docs/impl/`, closes the epic issue, then
   **deletes `docs/prd/epics/<epic-slug>/`**.

## Slice doc template (`slices/<n>-<slug>.md`)

```markdown
# Slice #<n> — <title>

**PRD:** ../prd.md · **kind:** feature|capability · **mode:** hitl|afk

## What to build
<end-to-end behaviour (feature) OR API surface + first consumer (capability)>

## Acceptance criteria
- [ ] …

## Blocked by
- #<n> — <reason>  |  None — can start immediately
<!-- also realised as a native `blocked_by` dependency in the tracker -->

## Test plan          ← appended by analyse-issue
…
```
