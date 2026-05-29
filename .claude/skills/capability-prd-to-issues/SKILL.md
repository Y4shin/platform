---
name: capability-prd-to-issues
description: Break a capability PRD (kind:capability) into independently-grabbable enabling slices (SDK/macro/host surface, each with a first consumer), create a PRD tracking issue that owns them, and write committed slice docs. Use after /create-capability-prd. Provider-aware (gh/fgj).
---

# Capability PRD → Issues

Convert a `kind: capability` PRD into independently-grabbable issues using **enabling
slices**, owned by a PRD tracking issue. Capabilities are foundational (no UI to demo), so
acceptance is a **consumer test**, not a demoable screen.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`** — !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ownership_note`

Per-provider commands come from `scripts/forge_detect.sh <key>`, injected at the step that
uses them. The PRD/artifact reference (frontmatter + `docs/prd/<slug>/` layout + lifecycle)
is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 0 — Provider + PRD

Verify auth, then locate the PRD at `docs/prd/<slug>/prd.md` and **assert `kind: capability`**
— if it's `feature`, stop and point at `/feature-prd-to-issues`.

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" auth_check`

Ensure the label scheme exists (idempotent):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ensure_labels`

## Step 1 — Explore (if needed)

Explore `crates/junius-sdk/` (+ `crates/junius-sdk-macros/` for macros), the host
`platform/`, and `docs/design/11-backend-plugin-interface.md` /
`12-frontend-plugin-interface.md`. Slice descriptions must respect the layering (host owns
auth/RBAC; plugins consume the SDK) and the `plugin.toml` manifest contract.

## Step 2 — Draft enabling slices

Break the PRD into **enabling slices**. Each slice is a thin unit of new API surface /
macro / host capability that **names its first real consumer**.

<enabling-slice-rules>
- Each slice ships a thin, coherent piece of the surface — prefer many thin slices.
- Each slice names a concrete first consumer (a plugin or host call site) so it isn't a
  speculative layer.
- Acceptance = a **consumer test**: a doctest / `#[cfg(test)]` unit in the crate **plus** a
  downstream consumer exercising it in an integration test; for macros, a
  `trybuild`/compile-fail test.
- A completed slice is independently mergeable.
</enabling-slice-rules>

Each slice is **HITL** (needs a human design decision) or **AFK** (autonomous). Prefer AFK.

## Step 3 — Quiz the user

Present the breakdown as a numbered list; per slice: **Title**, **Type (HITL/AFK)**,
**Blocked by**, **Surface + first consumer**. Ask: granularity right? dependencies correct?
does each slice name a real consumer? merge/split any? HITL vs AFK correct? Iterate until
approved.

## Step 4 — Publish (dependency order, blockers first)

Create-issue form for the detected provider:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_issue`

Attach a slice as a child of the PRD issue (detected provider):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_attach_subissue`

1. **Create the PRD issue** (label `prd`): body = PRD summary + an empty task list. Record
   its number as `prd_issue:` in `prd.md`.
2. For each slice, in dependency order:
   - Create the issue with labels `kind:capability`, `mode:hitl|afk`, `status:todo` (+
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
The API surface this slice adds (types / traits / fns / macro shape) **and its first
consumer**. Describe the surface precisely; inline a type/signature snippet when it encodes
the decision better than prose.

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer (<plugin/host>) exercises it in a test
- [ ] (macros) trybuild/compile-fail cases pass

## Blocked by
- #<n> — <reason>   (or "None — can start immediately")
</issue-template>

After publishing, report: `#<n> · <title> · HITL|AFK · consumer: <x> · blocked-by: …`, and
the PRD issue number.

## Constraints

- **kind:capability only** — abort on a feature PRD.
- **English**; **no speculative scope** — every surface names a consumer or it's deferred.
- Do not modify unrelated issues.
