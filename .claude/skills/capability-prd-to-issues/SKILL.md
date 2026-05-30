---
name: capability-prd-to-issues
description: Break a capability PRD (kind:capability) into independently-grabbable enabling slices (SDK/macro/host surface, each with a first consumer), wire the PRD issue + slices with native dependencies (and sub-issues under an epic), and write committed slice docs. Use after /create-capability-prd. Provider-aware (gh/fgj).
---

# Capability PRD → Issues

Convert a `kind: capability` PRD into independently-grabbable issues using **enabling
slices**, wired with the flat native tracker model (see the injected reference): the PRD issue
is `blocked_by` its slices; under an epic, the PRD and every slice attach as native sub-issues
of that epic. Capabilities are foundational (no UI to demo), so acceptance is a **consumer
test**, not a demoable screen.

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`** — !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" ownership_note`

Per-provider commands come from `scripts/forge_detect.sh <key>`, injected at the step that
uses them. The PRD/artifact reference (frontmatter + `docs/prd/<slug>/` layout + lifecycle)
is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 0 — Provider + PRD

Verify auth, then locate the PRD at `docs/prd/<slug>/prd.md` and **assert `kind: capability`**
— if it's `feature`, stop and point at `/feature-prd-to-issues`.

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" auth_check`

**Receipt guard:** expect a `create-capability-prd` receipt on this `prd.md` (its upstream); then
check `receipts:` for a prior `capability-prd-to-issues` entry (see **Run receipts** in the
injected reference). If present (or `status:` is already `issues-created`/later), the PRD was
already sliced — report "`capability-prd-to-issues` already ran on `<date>`" and confirm an
intentional re-run before re-creating issues.

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

## Step 4 — Publish (flat native model, dependency order, blockers first)

Read the injected reference for the tracker rule: **sub-issue = epic-only; dependencies =
everything else.** Check `prd.md` frontmatter for an `epic:` field — present ⇒ this PRD belongs
to an epic; absent ⇒ standalone.

Create-issue form for the detected provider:

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_issue`

Add a native dependency (make `<issue#>` blocked-by `<blocker#>`):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_add_dependency`

Attach a child as a sub-issue of the **epic** (only used when `epic:` is set):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_attach_subissue`

1. **Create the PRD issue** (labels `prd`, `kind:capability`): body = PRD summary. It is a
   regular issue — it does **not** own the slices as sub-issues. Record its number as
   `prd_issue:` in `prd.md`.
2. For each slice, in dependency order:
   - Create the issue with labels `kind:capability`, `mode:hitl|afk`, `status:todo` (+
     `milestone:M<NN>`). Body uses the template below (`Part of #<prd>` + blockers).
   - Add **PRD `blocked_by` this slice**.
   - For each blocker in the slice's `## Blocked by`, add **slice `blocked_by` blocker**.
   - Write `docs/prd/<slug>/slices/<n>-<slug>.md` from the template in `docs/workflow/artifacts.md`.
3. **If `epic:` is set:** attach the PRD issue **and every slice issue** as native sub-issues of
   the epic (`epic_issue:` from `docs/prd/epics/<epic-slug>/epic.md`). Add any **PRD `blocked_by`
   PRD** edges the epic's `prds[].blocked_by` calls for. Then in `epic.md`: set this PRD's
   `prds[].issue` to `<prd#>`, tick its checklist item on the epic issue, and set the epic
   `status: in-progress`.
4. Set `slices: [...]` and `status: issues-created` in `prd.md`, and add a
   `capability-prd-to-issues` entry to its `receipts:` (`on: <today>`, `note:` the PRD issue #)
   per **Run receipts**. Commit the `docs/prd/` changes (PRD dir, and the epic dir if touched).

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

After publishing, report: `#<n> · <title> · HITL|AFK · consumer: <x> · blocked-by: …` per
slice, the PRD issue number, and (if under an epic) the epic issue number with its updated
checklist.

## Constraints

- **kind:capability only** — abort on a feature PRD.
- **English**; **no speculative scope** — every surface names a consumer or it's deferred.
- **Tracker rule:** sub-issue parenting is epic-only; PRD↔slice and all ordering are native
  dependencies. The PRD issue never owns slices as sub-issues.
- Do not modify unrelated issues.
