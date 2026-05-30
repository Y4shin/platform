---
paths:
  - "**/*.proto"
---

# Proto / RPC conventions

Protos are managed with **buf** across the whole tree. Platform-level shared protos live in
`proto/` (`platform/v1`, `user/v1`); plugin-specific protos live in each plugin's `proto/`.

- **Format:** `buf format -w` (or `task fmt`).
- **Lint:** `buf lint`. Per-rule exemptions go in [buf.yaml](../../buf.yaml) with a comment
  explaining why (e.g. the `user.proto` exemption from `RPC_RESPONSE_STANDARD_NAME`).
- **Don't break the wire.** `task buf:breaking` runs `buf breaking` against `main`. Prefer
  additive changes (new fields/messages/RPCs); never renumber or repurpose existing field tags.
- Generated Rust/TS is produced by `buf generate` into `generated/` directories — edit the
  `.proto`, never the output.

RPC service/method shapes follow the conventions in
[docs/design/11-backend-plugin-interface.md](../../docs/design/11-backend-plugin-interface.md)
and the `junius-rpc-meta` crate.
