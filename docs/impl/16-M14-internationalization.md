# M14 — Internationalization

> **Status:** ✅ Implemented. The seam + Lingui integration + a full
> events-plugin retrofit ship together.
>
> Stages A–F land the host-side locale storage, the typed-codegen backend
> localizer, the library-agnostic FE seam, `junius i18n check` + CI gate, and
> the original hello retrofit. Stage G layered Lingui v5 on top (`t` /
> `<Trans>` / `useLingui` from `@lingui/react/macro`; the macros must be
> imported from a Lingui-recognised path, so the SDK re-exports only the
> runtime `i18n` singleton + `useLingui` + the `I18nProvider`). The
> three test plugins were then dropped, and the events plugin retrofitted
> end-to-end:
>
> - BE: `events/build.rs` calls `junius_i18n_build::generate`, `lib.rs`
>   invokes `i18n_catalog!()`, `register_i18n` installs the catalog;
>   `SendSignupConfirmation.recipient_locale` carries the recipient's locale
>   through the job queue; the email handler renders subject + body in EN/DE
>   via `messages::EventsSignupEmail{Subject,Body,RecipientFallback}`.
>   Three unit tests cover EN, DE, and the locale-specific empty-name
>   fallback ("there" / "zusammen").
> - FE: `plugins/events/frontend` ships 51 wrapped strings across
>   `EventsListPage`, `EventDetailPage`, `EventEditPage`, `InviteManagePage`,
>   `PublicInvitePage`, and `EventPicker`, with a `<Plural>` for "N slots
>   remaining" on the public invite page. Full DE translations committed.
>   `loadI18n` is exposed from the plugin index; the host shell wires it
>   into `<I18nProvider catalogs={[loadShellI18n, loadEventsI18n]}>`.
>
> `task ci:i18n` reports `events: ok (3 messages)`; `pnpm exec lingui
> extract` reports 5 host strings + 51 events strings, no missing translations.
>
> Open follow-ups (not blocking M14):
>
> - `junius sync` codegen for the FE catalog-loader array (today
>   `main.tsx` hand-lists it; same pattern as `generated/routes.ts`).
> - Playwright locale-switch spec ([e2e/locale.spec.ts](../../e2e/locale.spec.ts))
>   runs under `JUNIUS_E2E=1`; a CI-default unattended version is a future
>   enhancement.

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

## Library choices — locked in

| Choice | Decision | Rationale |
|---|---|---|
| **Frontend i18n library** | Lingui (deferred — Stage C ships the library-agnostic seam; Lingui plugs into it during the FE retrofit follow-up) | ICU under the hood, `.po` catalogs, compile-time macros for tiny runtime, Vite plugin |
| **Message format** | ICU MessageFormat (FE) · `{var}` substitution + distinct keys for plurals (BE) | FE keeps full ICU via Lingui; BE codegen rejects ICU plural/select syntax so the typed-struct fields stay 1:1 with placeholders |
| **Backend i18n approach** | Custom build-time codegen + a parsed `Template`/`Message` runtime in `junius-sdk` (no `polib` / `fluent-rs` dep) | Pure-Rust; catalogs baked into the binary as `&'static` arrays; lookups are O(1) `[Domain × Locale][ID]` indexing |
| **Catalog format + layout** | gettext `.po`, key-based (msgid = stable catalog key like `event.signup.subject`, msgstr = source-language template); `plugins/<name>/i18n/<locale>.po` + host `platform/i18n/<locale>.po` | Single format for both sides; Lingui supports `.po` natively |
| **Locale source priority** | `User.locale` (stored on `platform.user`) → `Accept-Language` → deployment-config `default_locale` (defaults to `"en"`) | Persisted on a column rather than a key/value table — simpler, indexable |
| **Locales shipped in M14** | `en`, `de`, `pseudo` | German is the first real translation; pseudo powers the CI hardcoded-string gate |

## Open questions — resolved

- **`junius check` vs `junius i18n check`** — kept as a separate subcommand (see
  [tools/junius/src/commands/i18n.rs](../../tools/junius/src/commands/i18n.rs)).
  Wired into CI as a fifth job (`task ci:i18n`, see [Taskfile.yml](../../Taskfile.yml)
  and [.github/workflows/ci.yml](../../.github/workflows/ci.yml)).
- **One catalog format across FE + BE** — yes, gettext `.po`. The build-time
  codegen and the Lingui runtime both consume the same files.

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
