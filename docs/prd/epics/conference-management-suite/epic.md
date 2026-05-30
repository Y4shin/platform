---
kind: epic
title: Conference management suite
slug: conference-management-suite
epic_issue: 34
prds:
  - slug: composition-contracts
    kind: capability
    issue: 35
    blocked_by: []
  - slug: composition-entity-links
    kind: capability
    issue: null
    blocked_by: [composition-contracts]
  - slug: composition-event-bus
    kind: capability
    issue: null
    blocked_by: [composition-contracts]
  - slug: agenda
    kind: feature
    issue: null
    blocked_by: [composition-entity-links, composition-event-bus]
  - slug: attendance
    kind: feature
    issue: null
    blocked_by: [agenda, composition-contracts, composition-event-bus]
  - slug: check-in
    kind: feature
    issue: null
    blocked_by: [attendance]
  - slug: speakers-list
    kind: feature
    issue: null
    blocked_by: [check-in, composition-contracts, composition-entity-links, composition-event-bus]
  - slug: motions
    kind: feature
    issue: null
    blocked_by: [speakers-list, composition-contracts, composition-entity-links, composition-event-bus]
  - slug: quorum
    kind: feature
    issue: null
    blocked_by: [motions, attendance, composition-contracts, composition-event-bus]
  - slug: voting
    kind: feature
    issue: null
    blocked_by: [quorum, composition-contracts, composition-entity-links, composition-event-bus]
  - slug: minutes
    kind: feature
    issue: null
    blocked_by: [voting, composition-contracts, composition-entity-links, composition-event-bus]
status: in-progress
---

# Conference management suite

## Problem / outcome

The platform can host an **event**, but it can't run the *proceedings* of one. A real
conference, congress, or assembly needs to manage who is present and eligible to act, what the
meeting will discuss, who gets to speak, what is being proposed and amended, and how decisions
are taken and recorded. Today none of that exists, and the natural shape — many specialised
tools that each do one job but reference each other constantly (a vote *on* a motion *under* an
agenda item, gated by *attendance*) — has no safe seam in the SDK to express those cross-plugin
links in a typed, optional-aware way.

**Outcome:** a coordinated suite of plugins that together let an organiser run a full assembly
on top of an existing `events` event — agenda, attendance, speakers lists, motions, voting,
quorum, check-in, and a composed minutes/protocol — where every plugin works standalone but
*composes* with the others when both are enabled. The connective tissue is a new SDK capability
that makes inter-plugin links **type-safe and gracefully absent** when an optional plugin isn't
deployed. The event row *is* the assembly; the agenda is its in-meeting spine; everything else
attaches to those two anchors.

## Constituent plugins & surfaces

- **`junius-sdk` / `junius-sdk-macros` / `junius` (host + codegen)** — the foundational
  inter-plugin capability (below). Cross-cutting; lands first.
- **`events` (existing)** — unchanged in shape; its `event` becomes the assembly container that
  every suite plugin hard-depends on. The suite reads `events.event`; events gains no knowledge
  of the suite.
- **`agenda` (new)** — the ordered list of agenda items for an event. The secondary spine: the
  thing motions, speakers lists, and votes optionally attach to.
- **`attendance` (new)** — the eligibility roster: who is present and eligible to act in a given
  session. Owns the **eligibility contract** (below) that the participatory plugins consume.
- **`speakers-list` (new)** — ordered queues of speakers under an agenda item, with extensible
  list *modes* (regular, quota-regulated, limited-slot random-pick).
- **`motions` (new)** — proposals, including **change-motions** (a motion that amends another)
  with the ordering/blocking rules that implies.
- **`voting` (new)** — decisions, with extensible *modes* (purely digital; analog with generated
  vote slips + count/duplication-prevention assistance). Optionally a vote *on* a motion.
- **`quorum` (new)** — computes and watches quorum off the live attendance roster; surfaces a
  quorum-status contract that voting/motions can gate on. **Hard-depends `attendance`.**
- **`check-in` (new)** — physical presence capture (QR/badge scan) that feeds the attendance
  roster. **Hard-depends `attendance`.**
- **`minutes` (new)** — the running protocol/record of the assembly, auto-composed from whatever
  of agenda/motions/voting/speakers-list is enabled. The all-optional aggregator.

## Shared / foundational work

This is the foundation; every plugin feature builds on it. It is **three separate capability
PRDs** so each primitive lands and is reviewed independently — `composition-contracts` first
(it establishes the shared manifest-schema + `junius sync` codegen + `junius check` framework),
then `composition-entity-links` and `composition-event-bus` extend that framework in parallel.

1. **`composition-contracts` — optional-aware backend service contracts (both directions).**
   The backend analogue of the existing frontend component registry
   ([08-cross-plugin-composition.md](../../../design/08-cross-plugin-composition.md)). Covers
   **two shapes**:
   - **1:1 optional contract** — a provider plugin declares a typed contract it implements; a
     consumer resolves it as `Option<impl Contract>`, **present only when the provider plugin is
     enabled**, optionality reflected in the type and verified against manifest deps at build
     time. Required-dep consumers see a concrete contract; optional-dep consumers see the
     `Option`. This is what lets `attendance` expose an **eligibility contract** (“is person P
     present & eligible for surface S in this session?”) and `quorum` expose a **quorum-status
     contract** that `speakers-list`/`voting`/`motions` consume without hard-depending on them.
   - **1:N extension slot** — an *aggregator* plugin declares a typed slot key once; **many**
     later plugins register a contribution into it; the aggregator enumerates whatever is
     present (empty until contributors arrive). This is the inverse of the pull-one-by-key
     frontend registry and is the mechanism that lets an **earlier** plugin surface **later**
     ones without being retrofitted (e.g. `agenda` declares an `agenda-item.attachment` slot
     that `motions`/`speakers-list`/`voting` fill as they land). See the backport policy under
     **Dependency ordering**.
2. **`composition-entity-links` — typed cross-plugin entity links.** A stable cross-plugin
   reference type plus the nullable-FK + resolver convention, so `motion → agenda item`,
   `vote → motion`, and `speakers-list → agenda item` are first-class, typed, and degrade (link
   reads as “unlinked”) when the target plugin is absent. Builds on the existing exposed-tables
   mechanism ([10-infrastructure-and-data.md](../../../design/10-infrastructure-and-data.md)
   §10.3) rather than replacing it.
3. **`composition-event-bus` — typed event/hook bus.** Typed pub/sub between plugins so reactive
   coordination is explicit and optional-aware: e.g. an `attendance` roster change notifies
   `quorum` and `speakers-list` to recompute eligibility; a `voting` result notifies `minutes`.
   Subscriptions to absent publishers are simply never delivered.

Each capability carries its share of the cross-cutting glue: manifest schema additions for
declaring exposed contracts/slots/events and consumed ones, `junius sync` codegen for the typed
registry/handles, and `junius check` rules (a consumer may only resolve a contract / fill a slot
/ subscribe to an event it declared a dep for; optional contracts must be consumed through the
`Option` surface).

**Eligibility vs RBAC.** The existing permission system stays the gate for *administering*
surfaces (static, role-based). Eligibility is a **separate, dynamic, per-session** concept —
“who may participate right now” — served by `attendance` over contract #1. The two are
deliberately distinct mechanisms.

**Extensibility is intra-plugin.** The speakers-list *modes*, voting *modes*, and motion
*rule-sets* are each a strategy trait + internal registry **inside their owning plugin** (add a
mode = add an impl). The foundational capability does **not** build a cross-plugin
mode-registration API in v1 (deferred — see Open questions).

## Per-plugin features

- **Inter-plugin capability** — the three SDK primitives + manifest/codegen/check support above.
  First consumer wired as part of its slices to prove the seam end-to-end.
- **agenda** — create/order/reorder agenda items under an event; item lifecycle (pending /
  active / closed); the anchor other plugins link to.
- **attendance** — the eligibility roster per session; mark present/eligible/excused; implements
  and exposes the eligibility contract; emits roster-change events.
- **speakers-list** — speaker queues attached to an agenda item; pluggable list modes (regular,
  quota-regulated, limited-slot random-pick) via an internal `SpeakerListMode` strategy
  registry; consumes eligibility (optional) to constrain who may enqueue.
- **motions** — motions and change-motions; enforce rules (no vote on a motion with outstanding
  change-motions; change-motion agenda-ordering constraints) via an internal rule-set strategy;
  optionally link a motion to an agenda item.
- **voting** — votes with pluggable modes (digital; analog with generated, de-duplicated slips
  + tally assistance) via an internal `VotingMode` strategy registry; optionally link a vote to
  a motion; optionally gate on quorum.
- **quorum** — quorum rules computed from the attendance roster (hard dep); exposes a
  quorum-status contract; reacts to roster-change events.
- **check-in** — QR/badge check-in flow that writes the attendance roster (hard dep).
- **minutes** — auto-composed protocol pulling from agenda/motions/voting/speakers-list
  (all optional) into an exportable record; driven by the event bus.

## Dependency ordering

The build order is **mandatory, not advisory**, and is chosen by **testability**: each plugin
lands after the plugins it links to, so its integration work is in-scope in its own PRD and is
e2e / manually testable against a plugin that already exists. Every step adds a demoable surface
*before* the web of plugins gets dense.

**Runtime deps stay optional; build order is mandatory.** A plugin still declares an *optional*
manifest dependency on a peer (graceful degradation when that peer is disabled at deploy — the
optional-aware SDK keeps earning its keep), but its PRD is **sequenced after** that peer so the
integration is written and tested for real. The `blocked_by` edges below encode the *development*
order, not the runtime dependency graph.

Mandatory order (each feature `blocked_by` its immediate predecessor + the capabilities it uses):

1. **`composition-contracts`** → **`composition-entity-links`** ∥ **`composition-event-bus`** —
   the interface foundation. No UI; each proven by its first consumer in the chain.
2. **`agenda`** — first testable UI: build & order an agenda on an event. The link anchor.
3. **`attendance`** — the eligibility roster; exposes the eligibility contract; emits roster
   events.
4. **`check-in`** — placed right after `attendance` so the roster is fully exercisable (scan →
   roster updates) before anything consumes eligibility.
5. **`speakers-list`** — first real composition: a queue on an agenda item where only
   present-eligible people may join (exercises entity-link → agenda + eligibility contract).
6. **`motions`** — motions + change-motions under an agenda item, with blocking rules.
7. **`quorum`** — placed before `voting` so voting can wire the quorum gate as in-scope
   integration; computed off the attendance roster.
8. **`voting`** — the big integration: vote on a motion, gated by quorum + eligibility.
9. **`minutes`** — last: the aggregator that composes whatever is enabled into a protocol.

### Backporting policy (prefer 1 > 2 > 3)

A linear order makes *later → earlier* references trivial but raises the question of how an
already-built plugin gains awareness of a later one. Three patterns, in preference order:

1. **Forward consumption (no backport).** Later plugin references earlier; the integration lives
   entirely in the later plugin's PRD. The common case (`voting → motions`, `minutes → all`).
2. **Inverted extension slot (no retrofit).** When an *earlier* plugin must surface *later*
   ones, it declares a typed **1:N slot** (`composition-contracts`) — and/or emits/subscribes on
   the **event bus** — *up front*; later plugins register contributions as they land; the early
   plugin's code never changes. This is what the **interface-first** emphasis buys: each early
   PRD enumerates the *kinds* of things that will later attach and declares slots for them.
3. **Explicit retrofit slice (backport).** When inversion is impractical, the **later** plugin's
   PRD owns a `retrofit <earlier-plugin>` slice that edits the earlier plugin directly — safe
   because the monorepo has no per-plugin versioning, cross-plugin edits are normal, and the
   mandatory order means the target is already stable. The cost is deliberately visible.

The interface-first review of each *early* PRD is the gate where we choose "declare a slot now
(2)" vs "accept a future retrofit (3)".

## Out of scope

- **Delegate / proxy voting** (vote delegation, effective-weight resolution) — considered and
  explicitly deferred; revisit as a follow-up PRD once `voting` lands.
- A **cross-plugin mode-registration API** (another plugin contributing a new voting/list mode
  via the SDK) — extensibility is intra-plugin in v1.
- A **dedicated assembly/sitting entity** distinct from the event (multi-day sittings, nested
  sessions) — the event is the assembly for v1.
- Changing the **`events`** plugin’s shape, or folding eligibility into the existing RBAC
  permission system.
- Real-time presence push transport details, hardware integration for badge scanners beyond the
  check-in capture flow, and external A/V or interpretation systems.

## Open questions

- The exact surface of the **event bus** (delivery guarantees: best-effort vs at-least-once;
  sync in-process vs job-backed) — to be pinned in the capability PRD’s design.
- Whether the **eligibility contract** is a single query interface or a richer capability set
  (eligibility + weight + speaking-rights) — resolved in the capability PRD with `attendance` as
  first consumer.
- Session granularity within an event: is a “session” an agenda item, a time window, or its own
  lightweight concept owned by `attendance`/`agenda`? — resolve when `agenda` + `attendance` are
  spec’d.
- Whether `minutes` export formats (PDF/HTML/structured) belong in v1 or a follow-up.
- Whether **delegates/proxy voting** should re-enter this epic or stand as its own epic.

## Decomposition

Epic issue: **#34**. Eleven child PRDs — **3 capabilities** (the interface foundation) then
**8 features** in a mandatory, testability-driven chain (see **Dependency ordering**). Each
child is created by its own `/create-(capability|feature)-prd` run (seeded with
`epic: conference-management-suite`) → `/*-prd-to-issues`, which attaches its PRD + slice issues
under epic #34 and fills the `issue:` fields above. Slices are where granularity lives; this
list is the PRD-level plan.

**Standing conventions for the child PRDs:**
- **Interface-first.** Every child leads with the interfaces it *exposes* (contracts, 1:N slots,
  link targets, events) and *consumes*, so cross-plugin fit is locked by declaration before
  behaviour is built.
- **Integration is in-scope.** A feature PRD includes the work to connect to every
  already-built plugin it links to (its predecessors in the chain).
- **Backport via slot, else retrofit slice.** Per the backport policy: prefer declaring a 1:N
  extension slot in the earlier plugin; otherwise the later plugin's PRD owns an explicit
  `retrofit <earlier-plugin>` slice.

| # | slug | kind | blocked_by | first testable surface |
|---|------|------|------------|------------------------|
| 1 | `composition-contracts` | capability | — | 1:1 contract + 1:N slot, proven by first consumer |
| 2 | `composition-entity-links` | capability | `composition-contracts` | typed cross-plugin reference resolves |
| 3 | `composition-event-bus` | capability | `composition-contracts` | typed publish → subscribe delivery |
| 4 | `agenda` | feature | `composition-entity-links`, `composition-event-bus` | build & order an agenda on an event |
| 5 | `attendance` | feature | `agenda`, `composition-contracts`, `composition-event-bus` | manage the eligibility roster |
| 6 | `check-in` | feature | `attendance` | scan/badge → roster updates live |
| 7 | `speakers-list` | feature | `check-in`, `composition-contracts`, `composition-entity-links`, `composition-event-bus` | queue on an agenda item; only present-eligible may join |
| 8 | `motions` | feature | `speakers-list`, `composition-contracts`, `composition-entity-links`, `composition-event-bus` | file a motion + change-motion under an item; blocking rules |
| 9 | `quorum` | feature | `motions`, `attendance`, `composition-contracts`, `composition-event-bus` | lose quorum → status flips; gates voting/motions |
| 10 | `voting` | feature | `quorum`, `composition-contracts`, `composition-entity-links`, `composition-event-bus` | vote on a motion, gated by quorum + eligibility |
| 11 | `minutes` | feature | `voting`, `composition-contracts`, `composition-entity-links`, `composition-event-bus` | run a mock assembly → composed protocol |
