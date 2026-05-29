---
name: feature-prd-to-issues
description: Break a feature PRD (kind:feature) into independently-grabbable issues as tracer-bullet vertical slices, create a PRD tracking issue that owns them, and write committed slice docs. Use after /create-feature-prd, or when converting a feature spec into work issues. Provider-aware (gh/fgj).
---

# Feature PRD → Issues

Convert a `kind: feature` PRD into a set of independently-grabbable issues using
**tracer-bullet vertical slices**, owned by a PRD tracking issue.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`** — !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ownership_note`

Per-provider commands come from `scripts/forge_detect.sh <key>`, injected at the step that
uses them. The PRD/artifact reference (frontmatter + `docs/prd/<slug>/` layout + lifecycle)
is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 0 — Provider + PRD

Verify auth, then locate the PRD at `docs/prd/<slug>/prd.md` and **assert `kind: feature`**
— if it's `capability`, stop and point at `/capability-prd-to-issues`.

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" auth_check`

Ensure the label scheme exists (idempotent):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ensure_labels`

## Step 1 — Explore (if needed)

If you haven't already explored the area, do so. Slice titles/descriptions must use the
project's domain glossary and layering (proto/RPC under `proto/`, migrations under
`migrations/`, plugin Rust in `plugins/<name>/src/`, frontend in `plugins/<name>/frontend/`),
respecting `docs/design/` and the `plugin.toml` manifest.

## Step 2 — Draft vertical slices

Break the PRD into **tracer bullet** issues. Each slice cuts a narrow but COMPLETE path
through every relevant layer end-to-end — NOT a horizontal slice of one layer.

<vertical-slice-rules>
- Each slice delivers a complete path: proto/RPC (Connect) → migration → plugin Rust →
  frontend route/component → test.
- A completed slice is demoable / verifiable on its own.
- Prefer many thin slices over few thick ones.
- Each slice has a concrete, single-test acceptance — pairs with `/analyse-issue` →
  `/implement-issue`.
</vertical-slice-rules>

Each slice is **HITL** (needs human interaction — a design/architecture decision) or
**AFK** (implementable + mergeable autonomously). Prefer AFK where possible.

## Step 3 — Quiz the user

Present the breakdown as a numbered list; per slice: **Title**, **Type (HITL/AFK)**,
**Blocked by**, **User stories covered**. Ask: granularity right? dependencies correct?
merge/split any? HITL vs AFK correct? Iterate until approved.

Create-issue form for the detected provider:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_issue`

Attach a slice as a child of the PRD issue (detected provider):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_attach_subissue`

1. **Create the PRD issue** (label `prd`): body = PRD summary + an empty task list. Record
   its number as `prd_issue:` in `prd.md`.
2. For each slice, in dependency order:
   - Create the issue with labels `kind:feature`, `mode:hitl|afk`, `status:todo` (+
     `milestone:M<NN>`). Body uses the template below, including `Part of #<prd>` and
     blockers referencing real issue numbers.
   - **Attach it as a child of the PRD issue** (snippet above).
   - Write `docs/prd/<slug>/slices/<n>-<slug>.md` from the template in `docs/workflow/artifacts.md`.
3. Set `slices: [...]` and `status: issues-created` in `prd.md`. Commit the `docs/prd/<slug>/`
   changes.

<issue-template>
## Part of
#<prd> (PRD: `docs/prd/<slug>/prd.md`)

## What to build
End-to-end behaviour of this vertical slice — not layer-by-layer implementation. Avoid file
paths / code snippets unless a snippet encodes a decision more precisely than prose (state
machine, schema, type shape).

## Acceptance criteria
- [ ] …

## Blocked by
- #<n> — <reason>   (or "None — can start immediately")
</issue-template>

After publishing, report: `#<n> · <title> · HITL|AFK · blocked-by: …`, and the PRD issue
number.

## Constraints

- **kind:feature only** — abort on a capability PRD.
- **English**; **no speculative scope** — surface gaps as questions, don't invent slices.
- Do not modify unrelated issues.
