# 13. Open Questions

These are decisions not yet taken. Each will need its own short doc or discussion before implementation.

- **Testing strategy.** Per-plugin unit tests are obvious. Integration tests across plugins? End-to-end browser tests? Contract tests for `.proto` files?
- **Asset handling.** Per-plugin static assets (images, fonts) — bundled with the FE or served separately?
- **Internationalization.** Where do translations live? Per plugin? Shared catalog?
- **Hot reload across plugin boundaries in dev mode.** Needs validation on a toy two-plugin setup before committing.
- **Multi-tenancy.** Does one platform binary serve one organization or many? Single-tenant (each org runs its own deployment) is the leaning default — matches the source/deployment split, keeps the data model simple — but needs to be made explicit. Multi-tenant changes every table (`org_id`), every Postgres role (RLS), and every query.
- **Audit logging.** Political-domain platforms need a verifiable "who did what, when" trail — for incident response, legal discovery, compliance. Decisions: what events are auditable, where stored (separate table per plugin? a host-wide `platform.audit_event`? a separate DB?), retention policy, who can read it, how plugins emit events through `junius-sdk`.
- **Trust model & threat model.** v1 implicitly assumes first-party plugins (all code is trusted, lives in the source monorepo or its submodules). This should be stated explicitly near [01-goals-and-constraints.md](01-goals-and-constraints.md), along with what changes if third-party plugins enter the picture later (capability enforcement at runtime, plugin sandboxing, code review process for external contributions).
- **Worked example & plugin authoring guide.** The doc currently references `docs/plugin-authoring-guide.md` in several places but it doesn't exist yet. A minimal "Hello, plugin" end-to-end (manifest, Rust handler, RPC, FE route, permission check, dev-mode run) would both validate the design and onboard plugin authors.
- **Implementation sequencing / v0 milestone.** Many moving parts (`junius`, `junius-sdk`, `@junius/sdk`, `@junius/generated`, `@junius/design`, Postgres roles, Connect-RPC interceptors, derive macros). A concrete v0 — one trivial plugin end-to-end with the minimum plumbing — would prove the architecture and prevent scope creep. Worth picking the v0 cut explicitly.
