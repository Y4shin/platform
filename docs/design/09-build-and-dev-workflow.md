# 9. Build & Dev Workflow

All workflows go through `junius`. See [07-junius.md](07-junius.md) for the full command reference.

## 9.1 Production build

```
junius check     # validate manifests + dep graph + drift
junius build     # → single binary at target/release/platform
```

## 9.2 Dev mode

```
junius dev
```

Orchestrates: Vite dev server (with HMR) + `cargo run` (with file watching) + `buf generate` on `.proto` changes + `junius sync` on `plugin.toml` changes. Vite proxies `/rpc/*` and `/h/*` to the backend. pnpm workspace symlinks make cross-plugin FE imports hot-reload.

(Dev mode ergonomics need real-world validation — flagged as open question.)
