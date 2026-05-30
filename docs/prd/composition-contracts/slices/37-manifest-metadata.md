# Slice #37 — Manifest + metadata for contracts/slots

**PRD:** ../prd.md · **kind:** capability · **mode:** afk

## What to build
- `junius-manifest`: `[exposes.contracts.<Name>]` (`iface`, `description`), `[exposes.slots.<Name>]`, and `contracts`/`slots` arrays on `[dependencies.<dep>]`.
- `junius-sdk` metadata: `ExposedContractDecl`, `ExposedSlotDecl`, and `contracts`/`slots` on `DependencyDecl`.
- `plugin_metadata!` macro emission of the new `'static` decls.

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: manifest-parse unit test round-trips the new sections; a fixture plugin asserts `metadata().exposed_contracts` / `.exposed_slots` / dep `contracts`
- [ ] additive: a manifest without the new sections still parses (serde defaults); `manifest_schema` unchanged

## Blocked by
- None — can start immediately
