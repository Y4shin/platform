---
kind: capability
title: "junius sync codegen"
slug: sync-codegen
issue: 41
prd: ../prd.md
mode: hitl
---

# Slice #41 — junius sync codegen

## What to build
`junius sync` codegen for the contract layer:
- Generate the immutable `Contracts` assembly (typed fields per enabled exposed contract/slot — concrete for 1:1, `Vec<Arc<dyn>>` for slots) from the enabled plugin set.
- Wire each enabled provider's `provide_contracts` into the boot build.
- Emit the per-consumer `dep_<provider>` cargo features the #39/#40 macros gate on.

**HITL:** settles the PRD open question — one shared generated `Contracts` crate vs per-consumer narrow modules (mirrors the FE `@junius/generated` question).

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: a `junius sync` snapshot test over a provider+consumer fixture; the generated assembly compiles
- [ ] the #39/#40 integration tests now run through the generated registry (not a hand-built one)
- [ ] re-running sync is idempotent

## Blocked by
- #39 — 1:1 macros consume the generated features/assembly
- #40 — slot macros consume the generated Vec fields
