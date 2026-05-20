# 2. Architecture at a Glance

```
  deployment dir                ┌────────────────────────┐
  platform.toml ─────────────▶ │   platctl (mgmt tool)  │
  (which plugins, config)      │   reads config + src,  │
                               │   composes + builds    │
                                └────────────────────────┘
                                       │
            ┌──────────────────────────┼──────────────────────────┐
            ▼                          ▼                          ▼
     Rust workspace             pnpm workspace             .proto schemas
     (backend crates)           (FE packages)              (Connect-RPC)
            │                          │                          │
            │                          ▼                          │
            │              Vite build → dist/                     │
            │                          │                          │
            ▼                          ▼                          ▼
     ┌────────────────────────────────────────────────────────────┐
     │   Single Rust binary                                       │
     │   - Axum server                                            │
     │   - Connect-RPC services (/rpc/<plugin>/*)                 │
     │   - Plugin HTTP routes (/p/<plugin>/* for non-RPC)         │
     │   - Embedded FE bundle served as static assets             │
     └────────────────────────────────────────────────────────────┘
```
