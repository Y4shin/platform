---
name: create-capability-prd
description: Interview the user to produce a capability PRD (foundational SDK/macro/host work with no UI) committed to docs/prd/<slug>/prd.md with `kind: capability` frontmatter. Use when introducing an API surface into junius-sdk, a new macro, a host capability, or infra primitive. Hands off to /capability-prd-to-issues.
---

# Create Capability PRD

Phase 0 of the **capability** track. Run the relentless `grill-me` interview, then
crystallise it into a committed PRD that `/capability-prd-to-issues` will slice. A
*capability* is foundational work others build on — a `junius-sdk` API surface, a
derive/attribute macro, a host capability, an infra primitive — with no UI to demo. For
user-facing plugin behaviour, use `/create-feature-prd` instead.

The PRD/artifact reference below is loaded via **dynamic context injection** (frontmatter
schema + `docs/prd/<slug>/` layout + lifecycle):

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Load context

Read `docs/design/11-backend-plugin-interface.md`, `docs/design/12-frontend-plugin-interface.md`,
`docs/design/08-cross-plugin-composition.md`, `crates/junius-sdk/` (and `crates/junius-sdk-macros/`
for macro work), and `clippy.toml`. Explore the codebase to answer your own questions
before asking the user.

## Step 2 — Grill (one question at a time)

Use the `grill-me` discipline. Always give your recommended answer + reasoning first, then
ask. Drive toward, in dependency order:

1. **API surface** — the exact shape (types, traits, fns, macro input/output). What does a
   consumer write?
2. **First real consumer** — which plugin or host code uses it first? (an unconsumed
   capability is speculative).
3. **Encapsulation** — what must NOT leak to plugins; what stays in the host
   (`platform/`) per the layering rules.
4. **Compatibility / versioning** — is this additive? Does it break existing plugins? Migration path?
5. **Boundaries** — out of scope; future extension points deliberately deferred.
6. **Acceptance** — how is each unit proven? (doctest + consumer integration test; for
   macros, `trybuild`/compile-fail).

If a question is answerable from the code/docs, answer it yourself and move on.

## Step 3 — Write the PRD

Write to `docs/prd/<slug>/prd.md` (`<slug>` = 3–5 word kebab of the title). Frontmatter per
`docs/workflow/artifacts.md` with `kind: capability` and `status: draft`. **If you were invoked
with epic context** (e.g. handed off from `/epic-to-prds` with an `epic: <epic-slug>`), set the
`epic:` field to that slug; otherwise omit it (standalone PRD). Body:

```markdown
# <title>

## Problem / why
## API surface
## First consumer
## Encapsulation & layering
## Compatibility / versioning
## Out of scope
## Open questions

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
```

Leave `prd_issue:` / `slices:` empty — `/capability-prd-to-issues` fills them.

## Step 4 — Hand off

Tell the user the PRD path and that it's ready for `/capability-prd-to-issues`. Don't
create issues here.

## Constraints

- **English**; **no speculative scope** — every surface must name a consumer or it's
  deferred to "Open questions".
- The PRD describes the surface + behaviour, not implementation file paths.
