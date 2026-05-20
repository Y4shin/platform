# 9. Build & Dev Workflow

All workflows go through `platctl`. See [07-platctl.md](07-platctl.md) for the full command reference.

## 9.1 Production build

```
platctl check     # validate manifests + dep graph + drift
platctl build     # → single binary at target/release/platform
```

## 9.2 Dev mode

```
platctl dev
```

Orchestrates: Vite dev server (with HMR) + `cargo run` (with file watching) + `buf generate` on `.proto` changes + `platctl sync` on `plugin.toml` changes. Vite proxies `/rpc/*` and `/h/*` to the backend. pnpm workspace symlinks make cross-plugin FE imports hot-reload.

(Dev mode ergonomics need real-world validation — flagged as open question.)
