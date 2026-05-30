---
name: create-epic
description: Interview the user to produce an epic — a coordinated outcome spanning several PRDs ("a set of plugins that do X") — committed to docs/prd/epics/<slug>/epic.md with `kind: epic` frontmatter. Use when the goal is bigger than one plugin/capability and needs to fan out into multiple PRDs. Hands off to /epic-to-prds.
---

# Create Epic

Phase −1 of the workflow: the tier **above** a PRD. Run the relentless `grill-me` interview,
then crystallise a higher-level outcome into a committed `epic.md` that `/epic-to-prds` will
decompose into ordered child PRDs. An *epic* is a coordinated set of plugins / cross-cutting
work delivering one outcome ("a set of plugins that do X"). For a single plugin feature use
`/create-feature-prd`; for one foundational capability use `/create-capability-prd`.

The PRD/artifact reference below is loaded via **dynamic context injection** (the three tiers,
frontmatter schema, `docs/prd/<slug>/` layout, tracker shape, lifecycle):

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Load context

Read `docs/design/02-architecture.md`, `docs/design/06-plugin-shape.md`,
`docs/design/08-cross-plugin-composition.md`, the `plugins/` trees that already exist, and any
related `docs/impl/` milestones. An epic almost always spans plugin boundaries and shared host
/ SDK work — understand the existing composition seams before asking the user.

## Step 2 — Grill (one question at a time)

Use the `grill-me` discipline. Always give your recommended answer + reasoning first, then ask.
Drive toward, in dependency order:

1. **Outcome** — the one-sentence result the whole epic delivers; who benefits.
2. **Constituent plugins & surfaces** — which plugins (new or extended) and host/SDK surfaces
   participate? Name each and its role in the outcome.
3. **Shared / foundational work** — what cross-cutting capability work (SDK, macro, host,
   manifest, navigation) must land **first** so the per-plugin features can build on it?
4. **Per-plugin features** — the user-facing behaviour each plugin contributes.
5. **Dependency ordering** — which pieces block which (capabilities before their consumers;
   plugin B depends on plugin A's exposed table/component).
6. **Boundaries** — what's explicitly out of scope; what must NOT change.

If a question is answerable from the code/docs, answer it yourself and move on.

## Step 3 — Write the epic

Write to `docs/prd/epics/<slug>/epic.md` (`<slug>` = 3–5 word kebab of the title). Frontmatter per
`docs/workflow/artifacts.md` with `kind: epic` and `status: draft`. Body:

```markdown
# <title>

## Problem / outcome
## Constituent plugins & surfaces
## Shared / foundational work
## Per-plugin features
## Dependency ordering
## Out of scope
## Open questions

## Decomposition
<!-- filled by epic-to-prds: the ordered child-PRD plan -->
```

Leave `epic_issue:` / `prds:` empty — `/epic-to-prds` fills them.

## Step 4 — Hand off

Tell the user the epic path and that it's ready for `/epic-to-prds`. Don't create issues or
child PRDs here.

## Constraints

- **English**; **no speculative scope** — anything not justified goes to "Open questions".
- The epic describes the outcome and the shape of its decomposition, not implementation file
  paths (those go stale and belong in the child PRDs / slices).
- An epic is optional sugar: if the outcome is genuinely one PRD, say so and point the user at
  `/create-feature-prd` or `/create-capability-prd` instead.
