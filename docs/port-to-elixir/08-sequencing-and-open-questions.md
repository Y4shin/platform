# 08 — Sequencing, Open Questions & Verification

> Part of the [Junius → Elixir/Phoenix report](README.md). Forward-looking.

## 1. Re-implementation sequencing

Mirror Junius's own milestone order — it was validated once already, and each step yields a
runnable system. Full parity target: **base platform + events + admin** ([README](README.md)).

1. **Host skeleton.** Phoenix umbrella/poncho; endpoint; Postgres; the `platform.*` + `meta.*`
   schema; `Junius.Plugin` behaviour + runtime registry ([05 §1–2](05-elixir-target-architecture.md));
   config loading + **boot-time manifest validation**; the shared UI OTP app (`junius_ui`).
2. **Auth.** OIDC (assent/oidcc) + Authentik; server sessions in Postgres; the `User` struct with
   memberships + user-roles; `has_permission?`; `LoadUser` plug + `Junius.Auth` `on_mount`;
   encrypted token storage (Cloak). ([05 §4](05-elixir-target-architecture.md))
3. **Authorization.** Port the `user_can_access` / `record_owner` / `forget_resource` SQL + the
   ownership/sharing tables **verbatim** ([03 §5](03-architecture.md)); the capability plug /
   `on_mount`; per-plugin `Ecto.Repo` on per-plugin Postgres roles + manifest-computed grants;
   runtime capability enforcement + the trusted-capability allowlist.
4. **Infra behaviours.** `Junius.Jobs` (Oban) / `Mailer` (Swoosh) / `Storage` (ExAws) /
   `Telemetry` (OTel → LGTM) / `Oidc` (assent). ([05 §8](05-elixir-target-architecture.md))
5. **`events` plugin end to end** in LiveView — schema/migrations, contexts joining the ACL,
   LiveViews + forms, the Oban confirmation-email job, iCalendar export, and **one
   collaborative surface via PubSub** (proves the live conference-manager pattern early).
6. **`admin` plugin** — groups/roles/user-roles/OIDC-mapping management + the permission
   catalogue (sourced from the registry) + audit log viewer.
7. **Cross-plugin composition** — context-function APIs + component registry + optional-dep
   graceful degradation ([05 §7](05-elixir-target-architecture.md)).
8. **Distribution** — the deploy-time assembler (igniter / `mess`-style) + the self-served-bundle
   option; `mix release` + Docker ([06](06-plugin-distribution-and-assets.md)).
9. **i18n** — Gettext, reusing the `.po` msgids; per-user locale.
10. **Provisioning + ops** — declarative provisioning task (port of `junius provision`);
    OTel → LGTM dashboards; the CI gate (Credo/Dialyzer/tests + assembler build).

## 2. Open questions to resolve while building

1. **Umbrella vs poncho vs single-app-with-plugin-deps.** Leaning **poncho + Hex deps** to make
   third-party plugins first-class naturally (umbrella assumes in-tree apps). Affects the
   assembler ([06](06-plugin-distribution-and-assets.md)).
2. **Router composition: build-time macro vs runtime route table.** The macro is simpler and the
   assembler rebuilds anyway; a runtime route table is only needed for the no-rebuild
   escape-hatch case ([05 §3](05-elixir-target-architecture.md)).
3. **Migration ordering.** Keep the Rust `@requires` DAG + per-plugin schema, or lean on Ecto's
   per-repo `migrations/` dirs with host-orchestrated ordering? Interacts with per-plugin
   repo/role auth — migrations still run as a privileged migrator role regardless
   ([05 §6](05-elixir-target-architecture.md)).
4. **Component-registry typing.** How much cross-plugin compile-time safety to recover —
   `@callback`-typed component contracts + Dialyzer vs. pure runtime lookup
   ([07 §5](07-mapping-and-tradeoffs.md)).
5. **Manifest source of truth.** Reuse the exact `plugin.toml` format (portable, matches the Rust
   validator) or move to an Elixir `manifest/0` returning a struct? Recommendation: keep
   `plugin.toml` for portability and a shared mental model, parse at boot.
6. **Where `platform.*` lives relative to plugin repos.** The host owns `platform.*` + `meta.*`;
   the migrator role owns the ACL functions. Confirm the plugin `Ecto.Repo`s all point at the
   same database (single-tenant) with distinct roles/search_paths.

## 3. Verification / de-risking plan

Because this deliverable is a report, "verification" means (a) the report is faithful to source,
and (b) the risky pieces of the new stack are proven early.

**(a) The report is accurate to source.** Every quoted interface in [03](03-architecture.md)–[05](05-elixir-target-architecture.md)
is copied from the cited path; re-check against the Rust repo when acting on it. The mapping in
[07 §1](07-mapping-and-tradeoffs.md) has no orphans (every host capability, manifest field, and
authz mechanism has an Elixir target).

**(b) Prove the risky pieces early, in this order:**

1. **Runtime plugin registry** — Phoenix host + a trivial `notes` OTP-app plugin
   ([05 §11](05-elixir-target-architecture.md)) that registers a LiveView route + a permission +
   an exposed component and appears at boot. This validates the single most load-bearing
   re-architecture (runtime composition replacing `junius sync`).
2. **The authz core** — port `user_can_access` + ownership/sharing into Ecto migrations and write
   tests reproducing every branch: owner, group-role, user-share, group-share, public, and the
   admin wildcard fast-path. Security-critical and language-neutral, so validate it in isolation.
3. **OIDC + sessions** against the existing dev Authentik (`dev/docker-compose.yml`) to confirm
   the identity flow re-implements faithfully (login, session cookie, groups-claim
   reconciliation).
4. **The asset pipeline** — two plugins each shipping Tailwind classes + a LiveView JS hook, run
   through the deploy-time assembler → one bundle, correct cache-busting, both hooks live. Then
   prove the self-served-bundle escape hatch with one plugin
   ([06](06-plugin-distribution-and-assets.md)).
5. **A collaborative surface** — a minimal LiveView + PubSub live-vote to validate the
   conference-manager pattern before committing the `events` plugin to it.

Each is a small, self-contained spike that retires a specific risk; do them before scaling up to
full events + admin parity.
