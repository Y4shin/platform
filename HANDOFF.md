# Handoff — M24 docker images, mid-pivot

**Last committed:** `3bbdc55` (CI fix sweep — predates the pivot). All
changes below are **uncommitted, working-tree only**.

## Mission

Get the three M24 deployment images (`junius-full`, `junius-backend`,
`junius-frontend`) building cleanly. The CI workflow
`.github/workflows/images.yml` matrix-builds them on every push to
`main` and publishes to `ghcr.io/<owner>/junius-<variant>:{latest,<sha>}`.

Backstory: the original Dockerfiles used `rust:1.88-bookworm` + apt
toolchain hunting and hit a wall — the project's `.cargo/config.toml`
mandates `lld`, the apt `binutils` metapackage didn't actually install
the linker, etc. **We pivoted to pure `nix build`**: the rust binaries
are produced by flake derivations (`packages.juniusd-static` /
`juniusd-headless-static` / `junius-static`), statically linked against
musl, runtime is `FROM scratch`. The FE bundles are also flake
derivations (`frontend-bundle`, `frontend-ssr-bundle`) using
`pnpm.fetchDeps` for hermetic dependency fetching.

## Status

| Image | State | Size | Notes |
|---|---|---|---|
| `junius-backend` | **✅ verified working** | 52 MB | Built locally, `--check-config` round-trips, trimmed `junius` CLI surface verified |
| `junius-full` | **❌ docker build fails** | — | `nix build .#juniusd-static` errors out in the nixos/nix sandbox at the cargo-vendor stage. Local `nix build .#juniusd-static` outside docker **succeeded** — 36 MB, SPA embedded (`<!doctype html>` strings present), statically linked. |
| `junius-frontend` | ⚠️ builds but huge | 2.73 GB | The runtime stage copies the entire pnpm workspace tree because `.pnpm` symlinks point at package sources (sdk/design/plugins/etc.). Functional but image size is unacceptable. |

## What works (verified locally)

```sh
# Backend — DONE
nix build .#juniusd-headless-static  # → /nix/store/.../bin/juniusd, 35 MB, static-pie
nix build .#junius-static            # → /nix/store/.../bin/junius, static-pie
docker build -f docker/backend.Dockerfile -t junius-backend-test .
docker run --rm -v <path>/platform.toml:/etc/junius/platform.toml:ro \
  -e OIDC_CLIENT_SECRET=x -e SESSION_KEY=x -e ROLE_PW_SECRET=x \
  junius-backend-test --check-config
# → "juniusd: config OK (/etc/junius/platform.toml)"

# Full — outside docker only
nix build .#juniusd-static           # → 36 MB, SPA embedded; verified via `strings`

# FE bundles — verified
nix build .#frontend-bundle          # → dist/{index.html,assets/}
nix build .#frontend-ssr-bundle      # → workspace tree, 498 MB on disk
```

## What's broken

### `docker build -f docker/full.Dockerfile`

Fails at `nix build .#juniusd-static` inside the `nixos/nix:2.24.10`
build sandbox. Truncated log (only the final 3 lines survived in
`/tmp/claude-1000/.../bvvlarn1n.output`):

```
264.2 error: 1 dependencies of derivation '/nix/store/…/vendor-registry.drv' failed to build
264.2 error: 1 dependencies of derivation '/nix/store/…/vendor-cargo-deps.drv' failed to build
264.3 error: 1 dependencies of derivation '/nix/store/…/juniusd-0.0.0.drv' failed to build
```

The same derivation builds fine **outside** docker (`nix build
.#juniusd-static` on the host). The difference: docker's buildx layer
runs nix under buildkit's sandbox, with no inherited `~/.cache/nix` or
substituter access from the host.

**Hypothesis (most likely):** the nixos/nix base image has substituters
restricted by default and the docker layer's sandbox can't fetch the
crane intermediate derivations (`vendor-registry`, `vendor-cargo-deps`).
Either:
- Need `--option substituters https://cache.nixos.org/` + the trusted
  user setup in the Dockerfile (`echo 'experimental-features = …\nsubstituters = https://cache.nixos.org/' >> /etc/nix/nix.conf`).
- Or the build is hitting network restrictions inside the docker layer
  (crane's `vendor-registry` does an http fetch of the cargo registry
  index even though Cargo.lock should make it offline-capable).

**Next diagnostic step:**

```sh
# Re-run with --keep-going + --print-build-logs to capture the real error:
nix build .#juniusd-static --print-build-logs --keep-going 2>&1 | tee /tmp/nix-full.log
# Or run the docker build with --progress=plain to keep all logs:
docker build --progress=plain -f docker/full.Dockerfile . 2>&1 | tee /tmp/docker-full.log
```

### `docker build -f docker/frontend.Dockerfile` is 2.73 GB

`frontend-ssr-bundle` derivation copies `packages/`, `plugins/`,
`platform/`, plus the entire `node_modules` (≈ 500 MB) to `$out` because
the pnpm workspace's `.pnpm/node_modules/@junius/*` symlinks point at
package source dirs. Without those source dirs, `noBrokenSymlinks`
fails the build.

**Possible fixes (in order of effort):**
1. **Prune `node_modules` to production-only:** add a `pnpm prune
   --prod` step in the derivation. Drops devDependencies (TypeScript
   types, lingui CLI, etc.). Should cut ~150 MB. But: `tsx` is in
   devDependencies of `@junius/shell-ssr` and the runtime needs it —
   move `tsx` to `dependencies` first.
2. **Bundle the SSR server itself:** use Vite/esbuild to produce a
   single-file `dist/server.js` that's NOT `node --import tsx
   src/server.ts` but a pre-bundled JS. Then the runtime image only
   needs `node + dist/`. Big win on size; requires writing a server
   bundle entry that handles Vite middleware-mode conditionally.
3. **Switch from full workspace copy to a `pnpm deploy` step:** `pnpm
   deploy --prod /out/runtime --filter @junius/shell-ssr` produces a
   self-contained dir with only what the package + transitive deps
   need. Cleanest; might still pull workspace packages but inlines them.

## File inventory (working tree, uncommitted)

### Authored during this session

| File | What changed | Status |
|---|---|---|
| `rust-toolchain.toml` | Added `targets = ["x86_64-unknown-linux-musl"]` | ✅ ready to commit |
| `flake.nix` | Added `crane` input + `packages.{juniusd-static,juniusd-headless-static,junius-static,frontend-bundle,frontend-ssr-bundle,pnpmDeps}`. Uses `pkgsCross.musl64` for the C-dep cross-toolchain (aws-lc-sys / zstd-sys need glibc-clean musl object files). `pnpm.fetchDeps` for hermetic JS deps with `fetcherVersion = 3` and hash `sha256-gbALDqmSIHDcjkQYjYwbTlg5u3yEsyh9mEa58g8i2gA=`. | ✅ ready |
| `docker/backend.Dockerfile` | Total rewrite. `FROM nixos/nix:2.24.10` builder → `nix build .#juniusd-headless-static .#junius-static` → `FROM scratch` runtime. | ✅ verified |
| `docker/full.Dockerfile` | Same shape as backend but `.#juniusd-static` (embeds SPA via `rust-embed`, FE bundle pulled in via the derivation's `postUnpack`). | ❌ fails in docker, ✅ works outside docker |
| `docker/frontend.Dockerfile` | `nixos/nix` builder runs `nix build .#frontend-ssr-bundle`, copies to `node:22-bookworm-slim` runtime, `node --import tsx src/server.ts`. | ⚠️ builds but 2.73 GB |
| `tools/junius/src/commands/sync.rs` | Added `skip_subprocesses` param to `apply_codegen`. The bundle-all path passes `true` so `pnpm exec buf generate` + `pnpm install` follow-ups are skipped (the precompiled-image build invokes them itself when needed). | ✅ ready |
| `flake.lock` | `crane` input lock entry added. | ✅ ready (auto-generated) |

### Authored by the user, parallel to this session (DO NOT REVERT)

| File | What | Status |
|---|---|---|
| `lefthook.yml` | **New file.** Pre-push git hook: `flake-fod-hashes` job runs `nix build .#pnpmDeps && nix build --rebuild --no-link .#pnpmDeps` whenever `flake.nix` or `pnpm-lock.yaml` is being pushed. Catches stale FOD hashes. | Untracked — user wants it in the repo. |
| `flake.nix` | User also added `pkgs.lefthook` to the dev shell's `packages` list (line 64 region). Intentional, supports the lefthook.yml. | Merged into my flake.nix edits. |
| `.gitignore` | User appended `/result` and `/result*` so the nix-build symlinks don't show up as untracked. | ✅ ready |

## Suggested next actions

1. **Diagnose the `full` docker build.** Easiest path: add `--option
   substituters 'https://cache.nixos.org/'` to the `nix build` calls
   in `full.Dockerfile`, OR copy `/etc/nix/nix.conf` from the host into
   the docker layer. Many `nixos/nix`-based Dockerfiles in the wild
   add a `RUN echo 'experimental-features = nix-command flakes
   extra-substituters = https://cache.nixos.org/' >> /etc/nix/nix.conf`
   shim — try that first.
2. **Shrink the `frontend` image.** Cleanest is option 2 above (pre-bundle
   the SSR entry with Vite). Lower-effort: option 3 (`pnpm deploy
   --prod`). Either should land the image under 700 MB.
3. **Once both work locally, commit + push.** The previous CI run
   (`3bbdc55`) showed lint + check both passed after the snapshot fixes;
   the only remaining red was images. So pushing the docker rewrites
   should turn CI fully green for the first time on M24.
4. **Cleanup before commit:**
   - Squash the M24 docker pivot into a single coherent commit
     (current branch has my failed attempts at binutils etc. as
     `d1f8538`/`3bbdc55` — those are already pushed to main, so
     commit the pivot as `fix: pivot M24 docker builds to nix +
     musl static binaries` referencing those.
   - Decide whether `lefthook.yml` + the `pkgs.lefthook` shellHook
     installation get the same commit or a separate one. (I'd
     separate — it's unrelated to M24.)

## How to reproduce locally

```sh
# Fresh shell, in the repo root.
cd /home/pplattner/Projects/platform

# 1. Reproduce backend success (~15 min cold, ~30s cached):
docker build -f docker/backend.Dockerfile -t junius-backend-test .
docker run --rm -v $PWD/examples/example-deployment-precompiled/full/platform.toml:/etc/junius/platform.toml:ro \
  -e OIDC_CLIENT_SECRET=x -e SESSION_KEY=x -e ROLE_PW_SECRET=x \
  junius-backend-test --check-config

# 2. Reproduce full failure (and capture full log this time):
docker build --progress=plain -f docker/full.Dockerfile -t junius-full-test . 2>&1 | tee /tmp/docker-full.log

# 3. Outside docker, full works:
nix build .#juniusd-static
file result/bin/juniusd       # static-pie linked
strings result/bin/juniusd | grep -E "<!doctype|<title>Junius"

# 4. Frontend builds, just too big:
docker build -f docker/frontend.Dockerfile -t junius-frontend-test .
docker images junius-frontend-test  # → 2.73 GB
```

## Context that matters

- `.cargo/config.toml` forces `-fuse-ld=lld` on linux-gnu targets. Doesn't
  apply to the musl target the docker builds use, so it's incidentally
  fine for the nix flow but a constant gotcha for any non-nix attempt.
- The flake's `juniusd-static` derivation uses `postUnpack` to copy the
  `frontend-bundle` output into `source/platform/frontend/dist/` so
  `rust-embed`'s build script finds it. The source filter for crane's
  `src` no longer excludes `dist` (was needed because rust-embed reads
  it; for the headless variant the dir is empty/absent → no-op).
- `pnpmDeps` hash is pinned. Whenever `pnpm-lock.yaml` changes, the build
  will fail with a `got: sha256-…` line; copy the new hash into
  `flake.nix:pnpmDeps`. The new `lefthook.yml` pre-push hook catches this.

## Outstanding M24 todos (from `~/.claude/projects/.../tasks/`)

1. ~~Pivot to nix+musl: rust-toolchain.toml + flake.nix derivations~~ ✅
2. ~~Rewrite docker/backend.Dockerfile with nix build + scratch runtime~~ ✅
3. **Rewrite docker/full.Dockerfile** — in progress; nix build succeeds outside docker, fails inside. Diagnose substituter/network access.
4. **Rewrite docker/frontend.Dockerfile** — builds work, image bloat needs addressing.
5. **Commit + push (with confirmation)** — pending the above.
