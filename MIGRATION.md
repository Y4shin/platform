# Migration: `docs/impl/` milestones → PRD workflow

Objective and strategy for moving the legacy linear milestone plan into the
feature/capability PRD workflow under [`.claude/skills/`](.claude/skills/). This is the
plan of record; nothing has been migrated yet (see **Status** below).

## Objective

Move planning of **unbuilt** work out of the sequential `docs/impl/` milestone format and
into the PRD workflow, so new work flows:

```
create-(feature|capability)-prd → (feature|capability)-prd-to-issues
  → analyse-issue → implement-issue (per slice) → finalize-prd
```

`finalize-prd` folds the durable knowledge back into [`docs/design/`](docs/design/) and
[`docs/impl/`](docs/impl/) and retires the transient PRD. See
[`docs/prd/README.md`](docs/prd/README.md), [`docs/workflow/forge.md`](docs/workflow/forge.md),
and [`docs/workflow/artifacts.md`](docs/workflow/artifacts.md).

## Framing: `docs/impl/` is the *destination*, not the source

In the new pipeline `finalize-prd` **writes** shipped records into `docs/impl/`. So the
existing milestone docs split in two, and only one half migrates:

- **Shipped (✅): M00–M18, M23, M24** — already in finalized-record form (each opens with
  `> Status: ✅ Implemented` plus deviation notes — exactly what `finalize-prd` produces).
  These **stay** as the permanent record. They are **not** migrated (turning shipped work
  into transient PRDs would be backwards).
- **Planned (🚧): M19, M20, M21, M22, M25** — forward-looking plans, i.e. PRDs written in
  the old milestone format. **These migrate.**

## How a milestone maps to a PRD

The old milestone template maps almost 1:1 onto the PRD + slices model:

| Milestone doc | PRD artifact |
|---|---|
| Stages (1..N) | slices (one issue each) |
| per-stage **Verify** | slice acceptance criteria / test plan |
| **Scope (out)** | PRD "out of scope" |
| **Library choices** | PRD decisions |
| **Goal / Why now** | PRD problem / why |

- `kind: feature` for user-facing milestones (M19, M20, M21); `kind: capability` for
  foundational ones (M22, M25).
- A trailing **"tests + docs" wrap-up stage is absorbed**, not turned into a slice:
  per-slice tests live in each slice's acceptance (via `analyse-issue`/`implement-issue`),
  and the doc/decision-log updates become `finalize-prd`'s output.

## Plan: pilot, then roll out

1. **Pilot — M19 (Core platform UIs).** Transcribe
   [`docs/impl/21-M19-platform-ui.md`](docs/impl/21-M19-platform-ui.md) into
   `docs/prd/m19-core-platform-ui/prd.md` (`kind: feature`, `milestone: M19`), linking back
   to the milestone doc for the full design rather than duplicating it. Run
   `/feature-prd-to-issues` to create the PRD tracking issue + the slice issues (M19 Stages
   1–6, with Stage 6 split into jobs/plugins/health) as its sub-issues, and write the slice
   docs. Add a migration banner to the M19 doc and point the `docs/impl/README.md` row at
   the PRD.
2. **Review the shape**, then repeat for **M20** / **M21** (`feature`) and **M22** / **M25**
   (`capability`).

## End state

- `docs/impl/` = shipped records + the `finalize-prd` destination; shipped milestones
  untouched.
- New planned work starts as a PRD via `/create-(feature|capability)-prd` — not a new
  `docs/impl/` doc.
- [`docs/impl/00-approach.md`](docs/impl/00-approach.md) §0.5 library-defaults table is kept
  as a living reference; its §0.2 "milestone doc template" is superseded by
  [`docs/workflow/artifacts.md`](docs/workflow/artifacts.md); the "defaults are not silent"
  rule is now carried by the grill/PRD interview.
- [`docs/impl/15-open-questions-resolution.md`](docs/impl/15-open-questions-resolution.md)
  stays as a design-level gating index that PRDs reference as blockers.

## Status

Pilot (M19) **executed — awaiting review.** Step 1 above is done:
[`docs/prd/m19-core-platform-ui/prd.md`](docs/prd/m19-core-platform-ui/prd.md) transcribes the
milestone (`kind: feature`, `milestone: M19`) and links back to the milestone doc for the full
design; `/feature-prd-to-issues` created the PRD tracking issue
([#1](https://github.com/Y4shin/platform/issues/1)) owning eight slice sub-issues (#2–#9 —
M19 Stages 1–6 with Stage 6 split into jobs/plugins/health, and Stage 7 "tests + docs"
absorbed); each slice has a committed `slices/<n>-*.md` spec; the M19 milestone doc carries a
migration banner and the `docs/impl/README.md` row points at the PRD.

The pilot was reviewed (LGTM) and the **rollout is complete** — every planned milestone is now
a PRD:

| Milestone | Kind | PRD | Tracking issue | Slice issues |
|---|---|---|---|---|
| M19 | feature | `docs/prd/m19-core-platform-ui/` | [#1](https://github.com/Y4shin/platform/issues/1) | #2–#9 |
| M20 | feature | `docs/prd/m20-app-navigation/` | [#10](https://github.com/Y4shin/platform/issues/10) | #11–#14 |
| M21 | feature | `docs/prd/m21-documentation-site/` | [#15](https://github.com/Y4shin/platform/issues/15) | #16–#20 |
| M22 | capability | `docs/prd/m22-rustfs-evaluation/` | [#21](https://github.com/Y4shin/platform/issues/21) | #22–#25 |
| M25 | capability | `docs/prd/m25-precompiled-followups/` | [#26](https://github.com/Y4shin/platform/issues/26) | #27–#30 |

Each milestone doc carries a migration banner pointing at its PRD, and the
[`docs/impl/README.md`](docs/impl/README.md) rows link the PRDs + tracking issues. The shipped
milestones (M00–M18, M23, M24) stay as permanent records — they are **not** migrated. New
planned work now starts as a PRD via `/create-(feature|capability)-prd`, not a new `docs/impl/`
doc. The migration objective is met; this document is retained as the rationale of record.
