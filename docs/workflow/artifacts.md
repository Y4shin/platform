# PRD + work-artifact reference

Shared by the `create-*-prd`, `*-prd-to-issues`, `analyse-issue`, `implement-issue`, and
`finalize-prd` skills. Defines the PRD frontmatter, the committed work directory, and the
lifecycle that keeps it self-cleaning.

## Canonical work directory (committed)

All planning artifacts live under `docs/prd/<slug>/`, version-controlled so any agent
picking up a slice issue reads the PRD + slice spec + test plan straight from the repo:

```
docs/prd/<slug>/
  prd.md                 # the PRD (frontmatter below) + an "## Implementation notes" log
  slices/
    <n>-<slug>.md        # one per slice/issue: spec + (after analyse-issue) "## Test plan"
```

`<slug>` is the PRD's `slug:`. `<n>` is the slice's issue number once it exists.

## PRD frontmatter

```yaml
---
kind: feature        # feature | capability — drives which prd-to-issues variant consumes it
title: <short human title>
slug: <kebab-slug>   # dir name + branch/issue slugs
milestone: M<NN>     # optional; links to a docs/impl/ milestone
prd_issue: <#n>      # filled in by *-prd-to-issues: the owning PRD tracking issue
slices: [<#a>, <#b>] # filled in by *-prd-to-issues: child (slice) issue numbers
status: draft        # draft | issues-created | in-progress | done
---
```

`feature-prd-to-issues` asserts `kind: feature`; `capability-prd-to-issues` asserts
`kind: capability`. Each refuses a mismatched PRD and points at the correct skill.

## Lifecycle / garbage collection

The slice docs and the PRD are **deleted as their work lands**, so the presence of a file
is itself state:

1. `create-(feature|capability)-prd` → writes `prd.md` (status `draft`).
2. `(feature|capability)-prd-to-issues` → creates the PRD issue + slice issues, writes one
   `slices/<n>-<slug>.md` spec per slice, fills `prd_issue:` + `slices:`, status
   `issues-created`.
3. `analyse-issue <n>` → appends a `## Test plan` section to that slice's doc.
4. `implement-issue <n>` → on completion, appends a note to `prd.md` `## Implementation
   notes`, then **deletes `slices/<n>-<slug>.md`**. A surviving slice doc ⇒ unfinished work.
5. `finalize-prd <slug>` → once `slices/` is empty, migrates durable knowledge into
   `docs/design/` + `docs/impl/`, closes the PRD issue, then **deletes `docs/prd/<slug>/`**.

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

## Test plan          ← appended by analyse-issue
…
```
