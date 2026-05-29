# PRDs & work artifacts

This directory is the **canonical, committed home for in-flight planning artifacts** —
PRDs and per-slice working docs — driven by the feature/capability workflow skills under
[`.claude/skills/`](../../.claude/skills/). It is intentionally **self-cleaning**: files
here represent work *not yet finished*, and they are deleted as work lands.

## Layout

```
docs/prd/<slug>/
  prd.md                 # the PRD + an "## Implementation notes" log
  slices/
    <n>-<slug>.md        # one per slice/issue: spec + (after analyse) "## Test plan"
```

## Two tracks

- **feature** (`kind: feature`) — user-facing plugin behaviour; broken into **vertical
  tracer-bullet slices** (proto/RPC → migration → plugin Rust → frontend → test).
- **capability** (`kind: capability`) — foundational SDK/macro/host work with no UI; broken
  into **enabling slices**, each named with its first consumer and proven by a consumer test.

## Lifecycle

1. `/create-feature-prd` or `/create-capability-prd` → writes `prd.md`.
2. `/feature-prd-to-issues` or `/capability-prd-to-issues` → creates a **PRD tracking
   issue** that owns the **slice issues**, and writes the slice docs.
3. `/analyse-issue <n>` → appends a `## Test plan` to the slice doc.
4. `/implement-issue <n>` → TDD + PR; on completion appends a note to `prd.md` and
   **deletes the slice doc**.
5. `/finalize-prd <slug>` → folds durable knowledge into `docs/design/` + `docs/impl/`,
   closes the PRD issue, and **deletes the whole PRD dir**.

So: a leftover `slices/<n>-*.md` means that slice is unfinished, and a leftover
`docs/prd/<slug>/` means the PRD isn't finalized yet. Permanent knowledge lives in
[`docs/design/`](../design/) and [`docs/impl/`](../impl/), never here.

See [`docs/workflow/artifacts.md`](../workflow/artifacts.md) for the frontmatter schema and
[`docs/workflow/forge.md`](../workflow/forge.md) for the issue/label/ownership conventions.
