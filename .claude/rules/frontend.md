---
paths:
  - "**/*.{ts,tsx,js,jsx}"
---

# Frontend conventions

Scope: `platform/frontend/` (`@junius/shell`), `packages/*`, `plugins/*/frontend/`. This is a
**pnpm** workspace on **Node 24** — use `pnpm`, never `npm`/`yarn`.

## Formatting & linting (biome)

Biome is the formatter and linter ([biome.json](../../biome.json)). Run `task fmt` or
`pnpm exec biome format --write .`; CI runs `biome ci`. House style:

- 2-space indentation, LF line endings, 100-column lines
- single quotes, semicolons always, trailing commas everywhere (`all`)

Typecheck with `pnpm run typecheck` (`tsc --noEmit` across the workspace).

## Don't edit generated output

Anything under a `generated/` directory (e.g. `platform/frontend/src/generated`,
`packages/generated/src`, `plugins/*/frontend/src/generated`) is produced by `buf generate` —
edit the `.proto` source instead. These are biome-ignored.

## i18n (Lingui)

User-facing strings are localized with Lingui. Compiled catalogs (`.po` → `.js`) are gitignored,
so run `pnpm exec lingui compile` before running FE tests that import them. The i18n gate
(`task ci:i18n`) fails on extract drift — re-extract when you add or change message strings.

## Shared libraries

Pull shared UI/tokens from `@junius/design`; use the generated RPC client + auth helpers rather
than hand-rolling Connect transport. See
[docs/design/12-frontend-plugin-interface.md](../../docs/design/12-frontend-plugin-interface.md).
