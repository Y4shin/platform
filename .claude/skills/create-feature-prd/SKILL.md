---
name: create-feature-prd
description: Interview the user to produce a feature PRD (user-facing plugin behaviour) committed to docs/prd/<slug>/prd.md with `kind: feature` frontmatter. Use when starting a new user-facing feature, turning an idea into a spec, or when the user says "let's spec a feature" / "write a PRD for this feature". Hands off to /feature-prd-to-issues.
---

# Create Feature PRD

Phase 0 of the **feature** track. Run the relentless `grill-me` interview, then crystallise
it into a committed PRD that `/feature-prd-to-issues` will slice. A *feature* is
user-facing plugin behaviour that cuts through the stack (proto/RPC → migration → plugin
Rust → frontend → test). For foundational SDK/macro/host work with no UI, use
`/create-capability-prd` instead.

The PRD/artifact reference below is loaded via **dynamic context injection** (frontmatter
schema + `docs/prd/<slug>/` layout + lifecycle):

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Load context

Read `docs/plugin-authoring-guide.md`, `docs/design/04-frontend.md`, the target plugin
under `plugins/<name>/` (or `docs/design/06-plugin-shape.md` if it's a new plugin), and any
related `docs/impl/` milestone. Explore the codebase to answer your own questions before
asking the user.

## Step 2 — Grill (one question at a time)

Use the `grill-me` discipline. Always give your recommended answer + reasoning first, then
ask. Drive toward, in dependency order:

1. **Who** is the user and **what** outcome do they get? (user stories)
2. **End-to-end behaviour** — the demoable path, not layer-by-layer implementation.
3. **Layers touched** — which plugin(s), proto/RPC services, migrations, frontend routes,
   background jobs?
4. **Permissions / ownership** — who may do this; user vs group principals (events plugin
   pattern).
5. **Boundaries** — what's explicitly out of scope; what must NOT change.
6. **Acceptance** — how do we know each piece works? (feeds the per-slice test strategy)

If a question is answerable from the code/docs, answer it yourself and move on.

## Step 3 — Write the PRD

Write to `docs/prd/<slug>/prd.md` (`<slug>` = 3–5 word kebab of the title). Frontmatter per
`docs/workflow/artifacts.md` with `kind: feature` and `status: draft`. **If you were invoked
with epic context** (e.g. handed off from `/epic-to-prds` with an `epic: <epic-slug>`), set the
`epic:` field to that slug; otherwise omit it (standalone PRD). Body:

```markdown
# <title>

## Problem / why
## User stories
## End-to-end behaviour
## Layers touched
## Out of scope
## Open questions

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
```

Leave `prd_issue:` / `slices:` empty — `/feature-prd-to-issues` fills them.

## Step 4 — Hand off

Tell the user the PRD path and that it's ready for `/feature-prd-to-issues`. Don't create
issues here.

## Constraints

- **English**; **no speculative scope** — anything not justified goes to "Open questions".
- The PRD describes behaviour, not file paths or code (those go stale).
