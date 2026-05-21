# Junius

A plugin-driven monolith for political work. The platform substrate provides user management, authentication, and shared infrastructure; each domain use case (speakers, events, canvassing, …) ships as a plugin that bundles its backend, frontend, and API contract together.

- **Architecture & design**: [docs/design/](docs/design/)
- **Implementation plan**: [docs/impl/](docs/impl/)

For the historical pre-split design doc, see [political-platform-design.md](political-platform-design.md).

## Getting a dev shell

The full toolchain (Rust, Node, pnpm, biome, buf) is provided by a Nix flake:

```bash
nix develop
```

With `direnv` installed, `direnv allow` once and the shell loads automatically when you `cd` in. From inside the shell, the standard `cargo` / `pnpm` / `biome` / `buf` invocations work as expected.
