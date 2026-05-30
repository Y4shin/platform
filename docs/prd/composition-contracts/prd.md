---
kind: capability
title: Cross-plugin contracts & extension slots
slug: composition-contracts
epic: conference-management-suite
prd_issue: 35
slices: [36, 37, 38, 39, 40, 41, 42]
status: issues-created
---

# Cross-plugin contracts & extension slots

## Problem / why

The platform has no typed, optional-aware way for one plugin to **call** another in-process.
Backend cross-plugin interaction today is limited to two mechanisms: Connect-RPC over declared
`[dependencies].rpc_methods`, and raw SQL against another plugin's `[exposes.tables]`. The typed,
gracefully-absent registry exists **only on the frontend** (`getComponent('venues.VenuePicker')`).

The conference suite needs in-process behavioural links constantly — "is person P eligible to
speak in this session?" (`speakers-list` → `attendance`), "is there quorum?" (`voting` →
`quorum`), "what attaches to this agenda item?" (`agenda` ← `motions`/`voting`/…). These are
synchronous questions answered against the provider's own data, called from inside another
plugin's request handler. RPC (serialization, async envelopes, proto DTOs) is the wrong weight
for them, and DB-table reads leak the provider's schema and bypass its invariants.

This capability adds the **backend analogue of the frontend component registry**: typed,
optional-aware, in-process plugin **contracts**, in two cardinalities:

- **1:1 contract** — one provider implements an interface; consumers resolve it. Used for
  `Eligibility`, `QuorumStatus`.
- **1:N extension slot** — one aggregator declares an extension point; many plugins contribute;
  the aggregator enumerates all enabled contributions. Used for `agenda-item.attachment` and the
  backport-free inversion pattern (an earlier plugin surfaces later ones without being edited).

Presence is resolved at **compile time** — `junius` controls the build, so the macro/`cfg`
layer knows exactly which plugins are linked into this deployment. Required deps resolve to a
**concrete** type (static dispatch, no `dyn`); optional deps collapse to a present/absent branch
chosen at compile time. The binary stays **stateless**: the contract registry is an immutable,
boot-built value and impls hold only cloneable resource handles, deriving every answer from the
DB per call.

## API surface

### 1. SDK safety traits (in `junius-sdk`)

```rust
/// Sealed marker every 1:1 contract interface trait must extend. Enforces the
/// object-/thread-safety bounds the host relies on when it builds and shares the
/// boot-time contract registry. Sealed: only `junius-sdk` types satisfy the
/// supertrait, so plugins extend `Contract` but cannot reimplement its guts.
pub trait Contract: Send + Sync + 'static {}

/// Sealed marker every 1:N slot *item* trait must extend (slot items are stored
/// heterogeneously as `Arc<dyn SlotItem-subtrait>`, so the bound is mandatory).
pub trait SlotItem: Send + Sync + 'static {}

/// Failure a provider's contract method may return to a consumer. Maps to
/// `ApiError`/`ConnectError` at the consumer's request boundary like `RepoError`.
#[derive(Debug, thiserror::Error)]
pub enum ContractError { /* Database, NotFound, Internal, … */ }
```

Contract/slot interface traits are `#[async_trait]` and object-safe (object-safety is required
for slots; harmless for 1:1).

### 2. Interface crates — `<provider>-contract`

Each provider that exposes contracts/slots ships a **lightweight, always-compiled** interface
crate holding only the trait(s) + plain DTOs — no DB, no heavy deps, **no dependency on any
`*-plugin` crate**. Both the provider impl crate and every consumer depend on it. Because it is
always in the build, `dyn Trait` and the trait's DTOs are nameable even in a deployment where the
*impl* provider plugin is disabled.

```rust
// plugins/attendance/contract/src/lib.rs   (crate: attendance-contract)
use junius_sdk::{Contract, ContractError};

pub struct EligibilityQuery { pub person: PersonId, pub surface: Surface, pub session: SessionId }
pub enum Surface { Speak, Vote, Propose }

#[async_trait::async_trait]
pub trait Eligibility: Contract {
    async fn is_eligible(&self, q: EligibilityQuery) -> Result<bool, ContractError>;
}
```

### 3. Provider registration — `Plugin::provide_contracts`

A new `Plugin` trait method with a **default empty impl** (so existing plugins are untouched).
The provider registers a boot-constructed `Arc` holding only cloneable resource handles; the
host calls this once at startup with the provider's own (caller-less) resources and the registry
is **immutable thereafter**.

```rust
fn provide_contracts(&self, reg: &mut ContractRegistrar<'_>) {
    // 1:1 — concrete impl, registered under its interface trait
    reg.contract::<dyn Eligibility>(Arc::new(AttendanceEligibility { db: self.db.clone() }));
    // 1:N — a contribution into another plugin's slot
    reg.slot::<dyn AgendaItemAttachment>(Arc::new(MotionsAttachment { db: self.db.clone() }));
}
```

Impls run under the **provider's system context** (the provider's Postgres role over the
provider's tables); the subject is passed explicitly in method args. A consumer's caller does
**not** need any of the provider's permissions — it is a system-level query the provider answers
about its own data.

### 4. Consumer resolution — compile-time-gated macros

`junius sync` builds an **immutable `Contracts` value at boot** — a *generated struct with typed
fields*, concrete for 1:1, `Vec<Arc<dyn SlotItem-subtrait>>` for slots — constructed from each
enabled provider's `provide_contracts`. Plugins never name or assemble it; they reach it through
macros whose expansion is gated by a `junius`-set cargo feature (`dep_<provider>`):

```rust
// REQUIRED 1:1 dep → concrete static dispatch, infallible (junius check guarantees presence)
let elig = contract!(attendance::Eligibility);          // : impl Eligibility (concrete)
let ok   = elig.is_eligible(query).await?;

// OPTIONAL 1:1 dep → present/absent branch chosen at compile time (no `dyn`, no unnameable type)
with_contract!(attendance::Eligibility, |elig| {
    elig.is_eligible(query).await?                       // block compiled only when enabled
}, else {
    true                                                 // graceful-degradation fallback
});

// 1:N slot (aggregator) → all enabled contributors, heterogeneous trait objects
for att in slot!(AgendaItemAttachment) {                 // : Vec<Arc<dyn AgendaItemAttachment>>
    if let Some(s) = att.summary(item_id).await? { render(s); }
}
```

- **Required 1:1** uses concrete static dispatch — no `dyn`, no `Option`.
- **Optional 1:1** uses the branching macro so the absent case never has to name the (absent)
  concrete type; the present branch sees the concrete type. Compile-time selected, dead branch
  eliminated.
- **Slots** are inherently dynamic-count + heterogeneous, so they remain `Vec<Arc<dyn …>>`. A
  contributor registers only when it is an enabled plugin, so the vector contains exactly the
  enabled contributors — no per-aggregator feature gate needed.

### 5. Manifest declarations

```toml
# provider exposes a 1:1 contract
[exposes.contracts.Eligibility]
iface = "attendance-contract"          # the always-compiled interface crate
description = "Is a person present & eligible for a surface in a session?"

# aggregator declares a 1:N slot (owns the slot item trait)
[exposes.slots.AgendaItemAttachment]
iface = "agenda-contract"
description = "A per-agenda-item summary another plugin can contribute."

# consumer of a 1:1 contract
[dependencies.attendance]
optional  = true
contracts = ["Eligibility"]

# contributor into an aggregator's slot
[dependencies.agenda]
optional = true
slots    = ["AgendaItemAttachment"]
```

### 6. `junius sync` + `junius check`

- **`junius sync`** generates the boot-built immutable `Contracts` assembly (typed fields per
  enabled exposed contract/slot), wires each provider's `provide_contracts`, and sets the
  per-consumer `dep_<provider>` cargo features the macros gate on.
- **`junius check`** rules: a `contracts`/`slots` entry must name a surface actually exposed by
  that dep; a consumer must depend on the dep's `*-contract` crate; an **optional** contract may
  only be reached through `with_contract!` (never the infallible `contract!`); a `*-contract`
  crate may not depend on any `*-plugin` crate; every `[exposes.contracts.X]` has a matching
  registration in `provide_contracts`; a slot contributor implements + registers the slot item
  trait.

## First consumer

The capability lands **before** its production consumers (`attendance`, `agenda`, `speakers-list`
are later PRDs), so each slice ships with its own proof:

- **SDK integration-test provider/consumer pair** — throwaway dummy plugins in the SDK/host test
  suite proving the mechanism end-to-end:
  - 1:1 **present** → `contract!` resolves the concrete impl and a method call returns its value.
  - 1:1 **absent** → `with_contract!` compiles to the `else` branch (feature off).
  - 1:N **two contributors** → `slot!` returns a vec of 2; **zero contributors** → empty vec.
  - `trybuild` compile-fail: resolving an undeclared contract; using `contract!` on an
    `optional = true` dep; a `*-contract` crate importing a `*-plugin` crate.
- **First production consumers** (delivered by the named later PRDs, against this interface):
  `attendance.Eligibility` consumed by `speakers-list`; `quorum.QuorumStatus` consumed by
  `voting`; the `agenda.AgendaItemAttachment` slot filled by `motions`/`speakers-list`/`voting`.

## Encapsulation & layering

- **`junius-sdk`** owns the generic primitives only: the sealed `Contract`/`SlotItem` markers,
  `ContractError`, the `ContractRegistrar` registration surface, and the `contract!` /
  `with_contract!` / `slot!` macros. It knows nothing about any domain contract.
- **`<provider>-contract` crates** are author-written, dependency-light, always compiled, and
  forbidden (by `junius check`) from depending on any `*-plugin` crate — the interface stays
  cheap to compile in every deployment.
- **The host (`platform/`)** owns assembly: it constructs each provider's impl with that
  provider's resources and builds the immutable `Contracts` value at boot. Plugins never build or
  name it; they only register (providers) and resolve via macros (consumers).
- **Statelessness:** the registry is immutable after boot; impls hold only `Arc` resource handles
  and derive answers from the DB per call. No in-memory caches or mutable state that would make
  the binary non-horizontally-scalable. Reviewed explicitly in acceptance.
- **Authority:** contract impls run under the provider's system context + Postgres role; the
  subject is an explicit argument. The consumer's caller is never elevated and never sees the
  provider's raw resources.

## Compatibility / versioning

Purely **additive**. New manifest sections (`[exposes.contracts]`, `[exposes.slots]`,
`[dependencies.*].contracts`/`.slots`) with serde defaults; a new `Plugin::provide_contracts`
method with a default empty impl (so `events`/`admin` and every existing plugin compile
unchanged); new SDK macros + traits; new generated `Contracts` module. No existing surface
changes shape. `manifest_schema` stays `1` (additive, optional sections). Single-version monorepo
— no semver on interface crates; the workspace tree is the version.

## Out of scope

- **The event/hook bus** (`composition-event-bus`) and **typed entity links**
  (`composition-entity-links`) — sibling capability PRDs.
- **Cross-plugin *mode* registration** (a plugin contributing a new voting/speakers-list mode) —
  deferred per the epic; mode strategies stay intra-plugin.
- **RPC/wire or frontend exposure of contracts** — contracts are in-process backend only; a
  consumer surfaces results through its *own* RPC/UI. No contract-over-the-wire.
- **Runtime hot-plugging** of providers — presence is a compile-time/per-deployment fact.
- **Interface semver / deprecation policy** — N/A under the single-version monorepo.

## Open questions

- Exact ergonomics of the optional 1:1 path: the `with_contract!` branching macro vs a generated
  newtype that makes `Option<Concrete>` always nameable — pick in the first slice.
- Whether the generated `Contracts` assembly is one shared crate or per-consumer narrow modules
  (mirrors the frontend `@junius/generated` question) — resolve during implementation.
- Telemetry/tracing propagation across the in-process contract-call boundary (spans should stitch
  consumer → provider).
- Whether a contract method ever needs the *consumer's* caller for audit attribution; if so, how
  it's passed without elevating authority.
- Whether slot contributions need a declared **order/priority** (e.g. agenda attachment display
  order) — defer until `agenda` needs it.

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
