# Slice #17 — Getting-started + glossary + CLI reference

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`23-M21-documentation.md` §C/E + Stage 2](../../../impl/23-M21-documentation.md#getting-started).

## What to build

The first reader-facing prose + the auto-generated CLI reference.

- Fill in `getting-started/quickstart.md` (clone → `nix develop` → docker → login → `task dev`,
  five-minute path), `prerequisites.md`, `first-plugin-tour.md`, and `reference/glossary.md`
  (one-line definitions of every term the design docs use undefined).
- Implement a new `junius docs cli --out <dir>` subcommand (`clap-markdown` / `clap_mangen`)
  that writes one fmt-clean markdown file per top-level `junius` command; wire
  `task docs:cli-reference` into `docs:build`; populate `reference/cli/*.md`.

## Acceptance criteria

- [ ] The quickstart walks a fresh contributor from clone to logged-in app in under five
      minutes.
- [ ] The glossary defines every term the design docs use without definition.
- [ ] `reference/cli/junius-sync.md` (etc.) match `junius <cmd> --help`.
- [ ] `task docs:check` detects drift when a `junius` flag changes without regenerating.

## Blocked by

- #16 — needs the book scaffold + section stubs + task targets.
