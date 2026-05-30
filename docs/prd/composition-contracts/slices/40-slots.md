# Slice #40 — 1:N slots (register_slot / slot!)

**PRD:** ../prd.md · **kind:** capability · **mode:** afk

## What to build
The 1:N slot mechanism:
- `ContractRegistrar::slot::<dyn S>(Arc<...>)` registration (accumulates a `Vec` per slot type).
- `slot!(<SlotItem>)` macro → `Vec<Arc<dyn SlotItem-subtrait>>` of all enabled contributors (a contributor registers only when it is an enabled plugin).

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: a dummy aggregator + two dummy contributors → `slot!` returns a vec of 2; with zero contributors → empty vec
- [ ] slot item trait bound to `SlotItem` (Send+Sync+'static) enforced

## Blocked by
- #37 — slot manifest declarations
- #38 — registrar/registry core
