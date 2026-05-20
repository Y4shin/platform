# 1. Goals & Constraints

- **Plugin-driven monolith.** One process, one deployable, but composed from independent plugin units.
- **Compile-time plugin composition.** Plugins are wired in at build time, not loaded dynamically. Adding/removing a plugin is a rebuild.
- **Plugins bundle backend + frontend.** A single plugin owns its server logic, API schema, UI routes, and any components it exposes to peers.
- **No JS/TS on the backend.** Hard constraint.
- **Significant SPA-like interactivity** alongside more CRUD-style views. The frontend stack must handle both regimes well.
- **End-to-end type safety** between backend and frontend.
- **Single self-contained deployment artifact.** One binary, no separate frontend hosting.
