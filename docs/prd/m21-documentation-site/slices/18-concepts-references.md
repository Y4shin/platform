# Slice #18 — Concepts + `plugin.toml` + HTTP-API reference

**PRD:** ../prd.md · **kind:** feature · **mode:** afk

Full design: [`23-M21-documentation.md` §C/Stage 3](../../../impl/23-M21-documentation.md#concepts).

## What to build

The synthesis pages that say *what a thing is, today, in one place* (the design docs say why;
the impl docs say when), plus two reference pages.

- The seven `concepts/*.md` pages: architecture map (one annotated Mermaid diagram),
  permissions (manifest → proto `requires` → SDK witness → SQL ACL → `has_permission_in_group`
  → admin wildcard), groups & roles, public surfaces, background jobs, i18n, apps & navigation
  (the last stubbed if M20 hasn't shipped).
- `reference/plugin-toml.md` — every key, what it means, when required, where read (matched to
  the `junius-manifest` crate fields).
- `reference/http-api.md` — every `/api/*` route + request/response shape + its session/role
  gate.

## Acceptance criteria

- [ ] The architecture map renders as a Mermaid diagram.
- [ ] The permissions concept page is the single page a new author needs (no chasing
      M06–M08 + M16 + M18).
- [ ] `reference/plugin-toml.md` matches the manifest crate fields one-to-one.
- [ ] `reference/http-api.md` lists every host `/api/*` route with its gate.

## Blocked by

- #16 — needs the book scaffold + section stubs.
