# Slice #36 — Contract/SlotItem markers + ContractError

**PRD:** ../prd.md · **kind:** capability · **mode:** afk

## What to build
The SDK safety foundation in `junius-sdk`:
- `pub trait Contract: Send + Sync + 'static {}` — sealed marker every 1:1 contract interface trait must extend.
- `pub trait SlotItem: Send + Sync + 'static {}` — sealed marker every 1:N slot *item* trait must extend.
- `pub enum ContractError` (`thiserror`) — provider-side failure surfaced to consumers; converts to `ApiError`/`ConnectError` like `RepoError`.

## Acceptance criteria
- [ ] surface compiles + doctest/unit passes
- [ ] first consumer: a `#[cfg(test)]` dummy `trait Foo: Contract` (+ DTO) compiles; Send+Sync bounds hold
- [ ] trybuild compile-fail: the sealed supertrait cannot be implemented directly by a plugin type
- [ ] `ContractError` converts to `ApiError` and `ConnectError`

## Blocked by
- None — can start immediately
