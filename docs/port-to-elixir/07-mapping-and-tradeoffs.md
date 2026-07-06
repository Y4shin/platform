# 07 — Rust → Elixir Mapping & Tradeoffs

> Part of the [Junius → Elixir/Phoenix report](README.md). Forward-looking. Consolidates the
> component mapping, what ports cleanly, what is knowingly given up, and the decision rationale.

## 1. Component mapping

| Junius (Rust) | Elixir/Phoenix target | Notes |
|---|---|---|
| `juniusd` Axum binary | Phoenix endpoint (Bandit), one OTP release | [05 §0](05-elixir-target-architecture.md) |
| Compile-time `plugins.rs` registry | **Runtime registry** (`Junius.PluginRegistry`) | Config-driven; boot-time validation replaces compile errors |
| `Plugin` trait | `Junius.Plugin` **behaviour** | [05 §1](05-elixir-target-architecture.md) |
| Connect-RPC + React SPA | **LiveView** views + a few controllers | Proto/Connect layer **dropped** |
| TanStack code-based routes | Phoenix Router composition (macro) | [05 §3](05-elixir-target-architecture.md) |
| `#[exposes.components]` + `useComponent` | LiveView components in a runtime registry | `{plugin, name} => module` |
| `plugin_metadata!()` type gen | **Runtime manifest parse + boot validation** | Undeclared usage → boot failure, not compile error |
| `Has<X>` type witnesses | **Runtime authz** (`on_mount` / plug + `User.has_permission?`) | The runtime half already exists in Rust |
| `#[repository]` + sqlx + `.sqlx` | **Ecto** contexts + per-plugin `Ecto.Repo` | On the plugin's Postgres role |
| `platform.user_can_access` etc. | **Ported verbatim** as Ecto migrations | Language-neutral; the security core |
| RabbitMQ jobs | **Oban** behind `Junius.Jobs` behaviour | Subsumes worker/DLQ/`job_run` |
| `lettre` email | **Swoosh** behind `Junius.Mailer` | SMTP default |
| `rust-s3` storage | **ExAws.S3** behind `Junius.Storage` | MinIO default |
| `tracing` + OTLP → LGTM | `:telemetry` + **OpenTelemetry** → LGTM | Same backend |
| OIDC (`openidconnect`) + Authentik | **assent / ueberauth_oidcc** + Authentik | Server sessions in Postgres; Cloak for tokens |
| `junius` CLI | **Mix tasks** + deploy-time **assembler** | [06](06-plugin-distribution-and-assets.md) |
| Lingui `.po` | **Gettext** (`.po`) | msgids reusable |
| `[exposes.tables]` SQL sharing | **Context-function APIs** | [05 §7](05-elixir-target-architecture.md); safer |
| Precompiled images / Nix / go-task | Mix **releases** + Docker | OTP releases are self-contained |

Every host capability handle from [04 §4](04-plugin-interface.md) maps to a behaviour or context
in [05 §8](05-elixir-target-architecture.md): `config` → app env / `Junius.Config`; `telemetry` →
`Junius.Telemetry`; `db` → per-plugin `Ecto.Repo`; `auth`/`users`/`groups` → `Junius.Accounts`;
`audit` → `Junius.Audit`; `authz` → `Junius.Authz`; `email` → `Junius.Mailer`; `jobs` →
`Junius.Jobs`; `storage` → `Junius.Storage`; `platform_admin` → `Junius.Admin` (trusted);
`localizer` → Gettext; `secrets` → config/`SecretStore`. **No orphans.**

## 2. What ports cleanly (language-neutral)

- The **PostgreSQL data model** — `platform.*` and `meta.*` tables ([03 §4](03-architecture.md)).
- The **`user_can_access` / `record_owner` / `forget_resource`** SQL functions — verbatim
  ([03 §5](03-architecture.md)). *The single most valuable thing to copy.*
- **Per-plugin schemas + least-privilege Postgres roles** and manifest-computed grants.
- The **up-only, `@requires`-ordered migration model** (or a close variant — see
  [08](08-sequencing-and-open-questions.md)).
- The **permission/capability model** — strings, groups, group-roles, global user-roles, the
  two-layer design, `resource_kind = "<plugin>:<table>"`.
- The **manifest schema and validation semantics** (`crates/manifest`).
- The **host-services model** (injected, capability-gated handles).
- The **frontend composition idea** (each plugin contributes routes + components).
- The **i18n catalogs** — Lingui and Gettext both use `.po`; msgids carry over.

## 3. What is knowingly given up — and how it's recovered

| Given up | Why it's Rust-only | Recovery in Elixir |
|---|---|---|
| **Compile-time proof that unauthorized code won't run** (`Has<X>`) | Requires the type system | Runtime permission checks (`on_mount`/plug + `User.has_permission?`) + tests. The check already exists as the runtime half in Rust |
| **Undeclared permission/secret/bucket = compile error** (`plugin_metadata!()`) | Compile-time macro | **Boot-time manifest validation** (fail fast) + Credo |
| **SQL only compiles inside repositories** (opaque `PluginDb`) | Type hiding | Convention: Ecto contexts + Credo rule + review |
| **Static end-to-end BE↔FE type safety** (proto/Connect) | Two languages, one schema | **One language** (LiveView) — the boundary disappears; arguably a net win |
| **"Missing host handle = compile error"** (constructor-less struct) | Rust structs | A plain struct + a test |

**The through-line:** Rust stacks a *static* enforcement layer on top of a *runtime* one. The
port keeps the runtime layer and drops the static one. We lose "the compiler catches it" and gain
a dramatically simpler system (no proto codegen, no `.sqlx`, no macro crates, no `junius sync`).
Safety is re-established with runtime checks + boot validation + tests + Dialyzer/Credo.

## 4. What gets *simpler* (net wins)

- **No composition codegen step.** OTP apps + a runtime registry replace generated `plugins.rs`,
  route trees, and component registries.
- **No proto/Connect layer.** LiveView collapses the BE↔FE type machinery into one BEAM process
  boundary — no `buf`, no `protoc-gen-es`, no witness-alias codegen, no dual-target generation.
- **No `.sqlx` cache, no proc-macro crates.**
- **Oban** replaces the RabbitMQ exchange/queue/DLQ/worker + `meta.job_run` bookkeeping with a
  library.
- **Realtime for free** — LiveView + PubSub make the collaborative surfaces (live conference
  manager) straightforward, versus building polling/subscription over RPC.
- **OTP supervision** replaces some lifecycle-hook ceremony.

## 5. What gets *harder* / needs care

- **Cross-plugin type safety.** Rust's narrow generated RPC namespaces and typed component
  registry gave compile-time cross-plugin checks. In Elixir this is runtime (registry lookups) +
  optional `@callback`-typed component contracts + Dialyzer.
- **Third-party plugin assets** genuinely require a build step per deployment (or self-served
  bundles) — see [06](06-plugin-distribution-and-assets.md). This is the one place the runtime
  model has a real, unavoidable cost.
- **The trust boundary is softer.** Capability enforcement is a guardrail, not a sandbox (§6).
- **SQL discipline** (repositories-only, always join the ACL) is now enforced by review/tests,
  not the compiler — worth a Credo check and a strong authoring guide.

## 6. Security posture (be explicit)

- **Layer 2 (resource ACL) is unchanged and remains the strong guarantee** — it's the same SQL,
  enforced by Postgres, and DB-level per-plugin roles still prevent a plugin from reading another
  plugin's tables directly. Cross-plugin context-function APIs ([05 §7](05-elixir-target-architecture.md))
  are *strictly safer* than the Rust exposed-tables model because the consumer role has no grants
  on the provider schema.
- **Layer 1 (capability/permission gate) moves from compile-time to runtime.** Equivalent
  enforcement for a *running* request; the loss is only the "won't compile" static check for
  *developers*, recovered by tests.
- **Runtime capability enforcement** (host handles) protects against *accidental* misuse and
  gives an audit/review surface for third-party plugins — but on the shared BEAM it is **not a
  hard sandbox** against malicious code. Document this; real isolation needs OS-process/node
  separation (out of scope for v1).

## 7. Decision log (rationale)

| Decision | Rationale |
|---|---|
| LiveView, drop Connect/React | Eliminates the proto/codegen/type-bridge machinery; unlocks realtime for the live conference manager; single language |
| Runtime/OTP plugins | Makes third-party plugins first-class; matches BEAM idioms; boot validation replaces compile-time checks |
| Oban behind a behaviour | Postgres-native (no RabbitMQ), durable + observable out of the box; behaviour keeps backends swappable |
| Context-function APIs for cross-plugin | Idiomatic; and DB-role isolation makes it strictly safer than shared SQL tables |
| Single-tenant, keep per-plugin roles | Matches current design; per-plugin roles are language-neutral defense-in-depth worth keeping even without compile-time confinement |
| Port the SQL ACL verbatim | Best-tested, security-critical, language-neutral; re-implementing it would be pure risk |
| Deploy-time assembler for assets | Best-quality output (shared design system); matches the release model; self-served bundles as a documented fallback |

Continue to [08 — Sequencing & open questions](08-sequencing-and-open-questions.md).
