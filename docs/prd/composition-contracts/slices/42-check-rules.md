---
kind: capability
title: "junius check rules"
slug: check-rules
issue: 42
prd: ../prd.md
mode: afk
---

# Slice #42 — junius check rules

## What to build
`junius check` validation rules for the contract layer:
- resolve of an undeclared contract/slot (must be declared on the named dep);
- `contract!` (infallible) used on an `optional = true` dep;
- a `*-contract` crate depending on any `*-plugin` crate;
- an `[exposes.contracts.X]` with no matching `provide_contracts` registration;
- a slot contributor that doesn't impl + register the slot item trait.

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: `junius check` fixtures (passing + failing) assert each diagnostic code fires
- [ ] clear error messages naming the offending plugin/contract

## Blocked by
- #41 — validates the fully-wired generated surface
