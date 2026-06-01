---
kind: capability
title: "1:1 resolution macros (contract! / with_contract!)"
slug: resolution-macros
issue: 39
prd: ../prd.md
mode: hitl
---

# Slice #39 — 1:1 resolution macros (contract! / with_contract!)

## What to build
The 1:1 consumer resolution macros in `junius-sdk-macros`:
- `contract!(<provider>::<Contract>)` — REQUIRED dep → concrete static dispatch, infallible.
- `with_contract!(<provider>::<Contract>, |c| { .. }, else { .. })` — OPTIONAL dep → present/absent branch chosen at compile time via the `dep_<provider>` cargo feature; present branch sees the concrete type (no `dyn`, no unnameable absent type).

**HITL:** settles the PRD open question — `with_contract!` branching macro vs a generated newtype making `Option<Concrete>` always nameable.

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: a dummy consumer plugin resolves a present provider (concrete value) and an absent one (`else` branch, feature off)
- [ ] trybuild compile-fail: `contract!` used on an `optional = true` dep; resolving an undeclared contract

## Blocked by
- #37 — manifest/feature declarations
- #38 — registry resolution primitive
