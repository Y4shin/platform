---
name: analyse-issue
description: Fetch a slice issue (#n), present a structured summary, then run a focused grilling session to decide the test strategy before any code. Appends a confirmed Test plan to the issue's committed slice doc and hands off to /implement-issue. Use when starting work on an issue, or when the user says "analyse"/"start on" #n. Provider-aware (gh/fgj).
---

# Analyse Issue

Phase 1: understand the issue, then challenge the developer to decide the right test
strategy *before* writing a line of code. The fastest honest feedback loop is the goal — a
slice must be testable, not just "covered".

Detected forge: **!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" git_type`**.
Per-provider commands come from `scripts/forge_detect.sh <key>`. The slice-doc/lifecycle
reference is injected below.

!`cat "$(git rev-parse --show-toplevel)/docs/workflow/artifacts.md"`

## Step 1 — Fetch + present

Fetch the issue (form for the detected provider):

!`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_get_issue`

Read its `kind:` label and `Part of #<prd>` to locate the PRD (`docs/prd/<slug>/prd.md`) and
the committed slice doc (`docs/prd/<slug>/slices/<n>-<slug>.md`); read both in full. Load
conventions: `docs/design/`, `docs/impl/README.md`, `docs/plugin-authoring-guide.md`,
`clippy.toml`.

Present:

```
Issue:   #<n>
Title:   <title>
Kind:    feature | capability        Mode: hitl | afk
PRD:     docs/prd/<slug>/prd.md

What needs to be built:
<2–4 sentences>

Touches:
- feature   → DB schema · proto/RPC · plugin Rust · frontend · job
- capability → API surface: <…> · first consumer: <…>
```

## Step 2 — Grill on test strategy (one question at a time)

Give your recommended answer + reasoning first, then ask. Work through in order; skip only
if a prior answer settles it:

1. "Which layers / surface does this slice touch end-to-end? (feature: list layers;
   capability: name the API surface + first consumer.)"
2. "What's the simplest test that gives honest confidence this works in *production*, not
   just in isolation?"
3. "If you run that test every few minutes while coding, is the feedback fast enough?"
4. "Walk me through the two most likely failure modes — does the test type catch both?"
5. "Do we need a real DB (testcontainers), real HTTP, or a real browser here? Is that cost
   worth it for this slice?"
6. "Is any part already tested elsewhere? What's the exact gap we're filling?"

**Test-type vocabulary** (pick the most confidence per minute):
- **e2e** — Playwright (`plugins/*/frontend/e2e/**/*.spec.ts`, `e2e/cross/**`) → `task test:e2e`
- **rust-integration** — testcontainers, `tests/*.rs`, `#[tokio::test]` → `task test:rust`
- **consumer-integration** — a plugin/host exercising a new SDK surface → `task test:rust`
- **rust-unit / doctest** — `#[cfg(test)]`, `///` examples → `task test:rust:unit`
- **macro compile-test** — `trybuild` / compile-fail → `task test:rust`
- **frontend-unit** — vitest `*.test.tsx` (jsdom) → `task test:js`
- **none** — only if truly trivial or fully covered elsewhere

**Push back when:** dev picks unit but the slice crosses DB/HTTP (→ integration/e2e is more
honest); dev picks e2e but the logic is pure (→ unit/doctest is faster); dev says "no test"
(→ drill in, accept only if trivial/covered); dev proposes several types (→ keep the one
with most confidence per minute unless they test genuinely different things).

## Step 3 — Persist the test plan to the slice doc

Once confirmed, **append** a `## Test plan` section to
`docs/prd/<slug>/slices/<n>-<slug>.md`:

```markdown
## Test plan

**Test type:** [e2e | rust-integration | consumer-integration | rust-unit/doctest | macro compile-test | frontend-unit | none]
**Reasoning:** <one sentence>

### Assertions
- <key assertion 1>
- <key assertion 2>
- <error cases: 400/401/403/404 or Err variants, if applicable>

### Test file
`<path, e.g. plugins/<name>/tests/<x>.rs or plugins/<name>/frontend/e2e/<x>.spec.ts>`

### Run command
`task test:rust` | `task test:rust:unit` | `task test:js` | `task test:e2e`
```

Commit the slice doc. Confirm the path to the developer.

## Hand-off

Invoke `/implement-issue <n>`. The spec + test plan live in
`docs/prd/<slug>/slices/<n>-<slug>.md`.

## Constraints

- **Spec-first** — assertions derive from acceptance criteria, never from an
  implementation.
- Don't start implementing here.
