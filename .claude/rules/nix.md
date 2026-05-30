---
paths:
  - "flake.nix"
  - "pnpm-lock.yaml"
---

# Nix flake & dependency hashes

The dev shell and CI toolchain come from [flake.nix](../../flake.nix). It contains
**fixed-output derivation (FOD) hashes** for fetched dependency caches — notably `.#pnpmDeps`,
driven by `pnpm-lock.yaml`.

When `pnpm-lock.yaml` changes (adding/updating a JS dependency), the `pnpmDeps` FOD hash in
`flake.nix` goes stale and must be re-pinned:

1. Run `nix build --no-link .#pnpmDeps` — it fails with a `got:` hash.
2. Copy that `got:` value into the `pnpmDeps` `hash = "...";` block in `flake.nix`.
3. Commit both files together (`build(nix): repin pnpmDeps hash …`).

The `lefthook` **pre-push** hook (`flake-fod-hashes`) verifies this on both cold and warm caches
and blocks the push on a mismatch, so a stale hash won't reach CI.
