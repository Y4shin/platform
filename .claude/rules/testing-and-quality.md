# Testing & code-quality strategy

## Test-driven, spec-first

New behaviour is built **red → green → refactor**:

1. **RED** — write the test first, deriving every assertion from the spec / acceptance criteria,
   **never** from the implementation. Run it and confirm it fails (a test that passes before the
   code exists is wrong).
2. **GREEN** — write the minimum to make it pass.
3. **REFACTOR** — clean up with the suite still green.

Never write a test to match a wrong implementation. Every acceptance criterion should map to at
least one test.

## Test taxonomy — pick the cheapest that proves the behaviour

| Kind | Where | Run with | Needs |
| --- | --- | --- | --- |
| Rust unit | `#[cfg(test)]` modules | `task test:rust:unit` | nothing (DB-less) |
| Rust integration | crate `tests/` | `task test:rust` | Docker (testcontainers) |
| FE unit | `*.test.ts(x)` (vitest) | `task test:js` (also typechecks) | nothing |
| E2E | `*.spec.ts` under `e2e/` (Playwright) | `task test:e2e` | Docker (ephemeral stack) |

Prefer unit over integration over E2E — reserve E2E for genuinely end-to-end flows. Tests must be
deterministic: no reliance on wall-clock, real network, or ordering beyond the provided
testcontainers stack.

## Quality gates (all enforced in CI)

- **clippy `-D warnings`** — zero warnings. Banned-API and `#[allow(...)]` exceptions need a
  `reason = "…"` at the narrowest scope (see [rust.md](rust.md)).
- **biome** — formatting + lint for TS/JS; no `biome-ignore` without a justification comment.
- **buf** — `lint` clean and no breaking proto changes vs `main`.
- **i18n** — no extract drift; catalogs complete.
- **`.sqlx` cache** — fresh (`cargo sqlx prepare --workspace` when queries change).

A PR is ready only when `task ci` is fully green with **zero skips**. Don't chase coverage
numbers — chase behaviour: a test exists because it proves an acceptance criterion or guards a
real regression, not to move a metric.
