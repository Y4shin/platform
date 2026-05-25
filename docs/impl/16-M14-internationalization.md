# M14 — Internationalization

> **Status:** 🚧 Planned — **top priority** (the first milestone after M13).
>
> *(This milestone is scoped at a high level; its **Library choices** are not yet
> decided. Detail it — like M12/M13 were — before implementing.)*

## Goal

Make the platform fully translatable end-to-end. Through M13 everything ships
**English-only** (and deliberately so — see
[15-open-questions-resolution.md](15-open-questions-resolution.md) §15.7). M14
delivers real internationalization across **both** the frontend and the backend,
plus the tooling to keep translations honest, and retrofits the existing plugins.

## Why now

i18n is the prioritized next milestone. It's far cheaper to land before the plugin
ecosystem grows: every plugin written after M14 is translatable from day one, and
the retrofit set is still small (`hello`, `greetings`, `widgets`, `events`).
Deferring it lets English strings calcify into many plugins and into backend
surfaces (emails, calendar exports) that a frontend-only shim can't reach.

## Scope (in)

### Frontend translation seam
- Adopt a real i18n library (decision below) behind a stable `@junius/sdk` surface:
  `t(key, vars?)`, a `<Trans>` component for rich/interpolated content, and a
  `useLocale()` hook. Plugin code depends only on the SDK seam, never the library
  directly.
- ICU-style message format (plurals, gender, interpolation), locale-aware
  number/date/currency via `Intl`.
- Per-plugin message catalogs, lazy-loaded with the plugin's routes.

### Backend translation seam (the part a frontend `t()` can't reach)
- A host-side localizer on the plugin context for strings that never touch the
  browser: **emails** (M10 transport), **iCalendar** `SUMMARY`/`DESCRIPTION`
  (M13 feeds/exports), and **validation / error messages**.
- Messages resolved against a **request/recipient locale**, not a global default.

### Locale resolution + preference
- A **per-user locale preference** (stored host-side), falling back to
  `Accept-Language`, then the deployment default.
- Token-authed calendar feeds (no `Accept-Language` reliably) and emails use the
  **subject/recipient's stored locale**.

### Tooling
- `junius i18n extract` — scan source for `t()` keys → per-plugin catalog.
- `junius i18n check` — a CI gate (fits the M12 four-job model): missing keys,
  untranslated entries, unused keys, and a **pseudo-locale** pass that proves no
  user-facing string is hardcoded (every visible string is wrapped).
- Catalogs live per plugin (`plugins/<name>/i18n/<locale>.<ext>`) + a host catalog.

### Retrofit + proof
- Migrate `hello`/`greetings`/`widgets`/`events` strings (frontend **and** backend)
  to keys.
- Ship at least one real non-English locale as proof, plus the pseudo-locale for
  testing. Update the plugin authoring guide's i18n section.

## Scope (out)

- **Machine translation / a translation-management UI or TMS integration.** Catalogs
  are edited as files; integrating a service is later work.
- **Full RTL layout polish.** The text seam lands; comprehensive right-to-left
  visual QA is deferred (noted as a follow-up).
- **Locale-specific content authoring** (e.g. per-locale event descriptions entered
  by users) — M14 translates *platform/plugin* strings, not user data.

## Library choices — confirm before starting

| Choice | Options | Notes |
|---|---|---|
| **Frontend i18n library** | `lingui` · `i18next`/`react-i18next` · `react-intl` (FormatJS) | Drives the catalog format + extraction tooling; all hide behind the SDK `t()` |
| **Message format** | ICU MessageFormat · Fluent | Affects plurals/gender + the catalog syntax |
| **Backend i18n approach** | `fluent-rs` · `gettext` · simple per-locale JSON/TOML catalogs keyed by message id | Must cover emails + iCalendar + errors; ideally shares the catalog format with the frontend |
| **Catalog format + layout** | e.g. `.po`, `.ftl`, or JSON; per-plugin `i18n/<locale>.<ext>` | Extraction/check tooling is built around this |
| **Locale source priority** | user preference → `Accept-Language` → deployment default | Where the per-user preference is stored (platform schema vs a settings table) |
| **Locales shipped in M14** | `en` + ≥1 real locale + a pseudo-locale | The pseudo-locale powers the "no hardcoded strings" CI check |

## Open questions

- Does `junius check` gain an `I18N.*` rule (missing/untranslated keys), or does
  `junius i18n check` stay a separate gate? (Lean: separate command, wired into CI.)
- One catalog format across FE + backend, or two? (Prefer one to share tooling.)

## Verification

```bash
# Switch locale (user preference) → the UI renders translated.
# An email triggered for a non-English user arrives in their locale (mailpit).
# An iCalendar feed/export for a non-English subject has translated VEVENT text.
target/release/junius i18n extract          # catalogs updated from source
target/release/junius i18n check            # CI gate: no missing/untranslated/unused keys
#   - run against the pseudo-locale → every visible string is wrapped (no leaks)
cargo test --workspace
pnpm exec playwright test                   # locale-switch E2E
```

After M14, every existing and future plugin is translatable across UI, email, and
calendar surfaces, and CI keeps catalogs in sync.
