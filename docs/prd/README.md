# PRDs & work artifacts

This directory is the **canonical, committed home for in-flight planning artifacts** —
PRDs and per-slice working docs — driven by the feature/capability workflow skills under
[`.claude/skills/`](../../.claude/skills/). It is intentionally **self-cleaning**: files
here represent work *not yet finished*, and they are deleted as work lands.

## Layout

```
docs/prd/epics/<epic-slug>/
  epic.md                # kind: epic — the epic brief + decomposition (no slices/)

docs/prd/<prd-slug>/
  prd.md                 # the PRD + an "## Implementation notes" log
  slices/
    <n>-<slug>.md        # one per slice/issue: spec + (after analyse) "## Test plan"
```

## Three tiers

- **epic** (`kind: epic`) — a coordinated outcome across several PRDs ("a set of plugins that
  do X"). *Optional* — a lone PRD needs no epic. Broken into **child PRDs**.
- **feature** (`kind: feature`) — user-facing plugin behaviour; broken into **vertical
  tracer-bullet slices** (proto/RPC → migration → plugin Rust → frontend → test).
- **capability** (`kind: capability`) — foundational SDK/macro/host work with no UI; broken
  into **enabling slices**, each named with its first consumer and proven by a consumer test.

The tracker is **flat**: an epic is the only sub-issue parent (it owns its child PRD *and*
slice issues); every other relationship — PRD `blocked_by` its slices, slice ordering, cross-PRD
order — is a native issue **dependency**. See [`../workflow/forge.md`](../workflow/forge.md).

## Lifecycle

1. *(optional)* `/create-epic` → writes `epic.md`; `/epic-to-prds` → creates the **epic
   issue**, plans the ordered child PRDs, hands off to the per-child create skills.
2. `/create-feature-prd` or `/create-capability-prd` → writes `prd.md` (carries `epic:` when
   created under an epic).
3. `/feature-prd-to-issues` or `/capability-prd-to-issues` → creates the **PRD issue** + the
   **slice issues**, wires native sub-issues (under an epic) + dependencies, writes slice docs.
4. `/analyse-issue <n>` → appends a `## Test plan` to the slice doc.
5. `/implement-issue <n>` → TDD + PR; on completion appends a note to `prd.md` and
   **deletes the slice doc**.
6. `/finalize-prd <slug>` → folds durable knowledge into `docs/design/` + `docs/impl/`,
   closes the PRD issue, and **deletes the whole PRD dir**.
7. *(optional)* `/finalize-epic <slug>` → once every child PRD is finalized, folds epic-level
   knowledge into `docs/design/` + `docs/impl/`, closes the epic issue, **deletes the epic dir**.

So: a leftover `slices/<n>-*.md` means that slice is unfinished, a leftover `docs/prd/<prd-slug>/`
means the PRD isn't finalized, and a leftover `docs/prd/epics/<epic-slug>/epic.md` means the
epic isn't finalized. Permanent knowledge lives in [`docs/design/`](../design/) and
[`docs/impl/`](../impl/), never here.

See [`docs/workflow/artifacts.md`](../workflow/artifacts.md) for the frontmatter schema and
[`docs/workflow/forge.md`](../workflow/forge.md) for the issue/label/ownership conventions.
