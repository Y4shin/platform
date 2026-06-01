---
kind: capability
title: "Registration surface + immutable registry core"
slug: registry-core
issue: 38
prd: ../prd.md
mode: afk
---

# Slice #38 — Registration surface + immutable registry core

## What to build
The registration surface + immutable registry core in `junius-sdk`:
- `Plugin::provide_contracts(&self, reg: &mut ContractRegistrar<'_>)` — new trait method with a default empty impl.
- `ContractRegistrar` with `contract::<dyn T>(Arc<...>)` registration.
- The immutable registry primitive the generated `Contracts` struct will wrap (boot-built, immutable after construction; impls hold only `Arc` handles).
- Establishes the `<provider>-contract` interface-crate convention (used by the test fixtures).

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: an SDK integration test registers a dummy 1:1 impl (from a `<dummy>-contract` crate) via `provide_contracts`, builds the registry, resolves it, calls a method → returns the value
- [ ] existing plugins (events/admin) compile unchanged against the default-empty hook
- [ ] registry is immutable post-boot (no interior mutability that breaks statelessness)

## Blocked by
- #36 — needs the `Contract` marker
