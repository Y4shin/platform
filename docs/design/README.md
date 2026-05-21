# Junius — Design Docs

A plugin-driven monolith for political work. The core platform provides user management, authentication, and shared infrastructure; each domain use case (e.g. speakers, events, canvassing) is delivered as a plugin that bundles its backend, frontend, and API contract together.

This directory captures decisions taken so far. Open questions are listed in [13-open-questions.md](13-open-questions.md).

## Index

| # | Document | What it covers |
|---|---|---|
| 1 | [Goals & Constraints](01-goals-and-constraints.md) | The hard requirements driving every other choice. |
| 2 | [Architecture at a Glance](02-architecture.md) | One-page sketch of how the pieces fit together. |
| 3 | [Backend](03-backend.md) | Rust + Axum host, plugin contract, Connect-RPC API. |
| 4 | [Frontend](04-frontend.md) | React + TanStack Router stack, plugin contract. |
| 5 | [Repository & Deployment Layout](05-repository-and-deployment-layout.md) | Source monorepo structure and the deployment-vs-source split. |
| 6 | [Plugin Shape](06-plugin-shape.md) | Per-plugin directory layout and the `plugin.toml` manifest. |
| 7 | [The Management Tool (`junius`)](07-junius.md) | The CLI that composes, syncs, scaffolds, builds, and runs dev mode. |
| 8 | [Cross-Plugin Composition](08-cross-plugin-composition.md) | Required vs optional inter-plugin deps and the typed component registry. |
| 9 | [Build & Dev Workflow](09-build-and-dev-workflow.md) | The end-to-end production build and `junius dev` flow. |
| 10 | [Infrastructure & Data](10-infrastructure-and-data.md) | DB, storage, jobs, email, telemetry, migrations, authn/authz. |
| 11 | [Backend Plugin Interface](11-backend-plugin-interface.md) | The Rust API surface plugin authors write against. |
| 12 | [Frontend Plugin Interface](12-frontend-plugin-interface.md) | The TypeScript/React surface plugin authors write against. |
| 13 | [Open Questions](13-open-questions.md) | Decisions not yet taken. |
| 14 | [Decision Log](14-decision-log.md) | Dated record of every locked-in decision. |
