{
  description = "Junius — plugin-driven monolith for political work";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # M24: crane is the nix-native Rust derivation builder. The
    # `docker/{backend,full}.Dockerfile` precompiled-image builds invoke
    # `nix build .#…` against the outputs defined below to produce
    # statically-linked musl binaries on a `FROM scratch` runtime image.
    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, crane }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      # Rust toolchain — single source of truth in `rust-toolchain.toml`.
      # `targets = ["x86_64-unknown-linux-musl"]` there gives us the
      # musl cross-target without needing to override here.
      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          rustToolchain = rustToolchainFor pkgs;

          # `fgj` — Forgejo/Codeberg client CLI (issues, PRs, Actions),
          # the `gh` equivalent for our Codeberg-hosted forks/mirrors.
          # NOT in nixpkgs: nixpkgs ships `forgejo-cli` (the `fj` tool),
          # which lacks Actions support — so we build romaintb/fgj from
          # source here. Plain `go build`; version is baked into
          # cmd/root.go, so no ldflags. Bump `version` + both hashes to
          # upgrade (vendorHash via `lib.fakeHash` + rebuild to discover).
          fgj = pkgs.buildGoModule rec {
            pname = "fgj";
            version = "0.4.0";
            src = pkgs.fetchFromGitea {
              domain = "codeberg.org";
              owner = "romaintb";
              repo = "fgj";
              rev = "v${version}";
              hash = "sha256-7/ITo+8QCj/hy4xlOw+kfjnJbHTWjGh+VYOZxvqghAQ=";
            };
            vendorHash = "sha256-ZBdSSif9YFpFyBQNpZ/XttVw/dgDS54L+0ZA+9ObSSg=";
            # Functional tests need a live Forgejo instance; skip in build.
            doCheck = false;
            meta.mainProgram = "fgj";
          };
        in {
          default = pkgs.mkShell {
            packages = [
              rustToolchain

              # Forge CLIs for the feature-workflow skills (.claude/skills/): the
              # skills detect the git remote and use `gh` for GitHub or `fgj` for
              # Forgejo/Codeberg. `fgj` is the derivation above; `gh` ships in nixpkgs.
              fgj
              pkgs.gh

              # JS / TS toolchain — versions pinned in package.json + rust-toolchain.toml
              # match what's expected in CI.
              pkgs.nodejs_24
              pkgs.pnpm

              # Linters / formatters / proto tooling. buf is installed here so junius dev
              # mode (M05+) doesn't need to vendor it on developer machines.
              pkgs.biome
              pkgs.buf
              # protoc is invoked by prost-build during `cargo build` of any
              # crate that depends on .proto-generated message types
              # (every plugin from M05 onward).
              pkgs.protobuf

              # Native dev tools — needed for git workflows, sqlx, etc.
              pkgs.git
              pkgs.openssl
              pkgs.pkg-config

              # Git hooks managed by `lefthook.yml` at the repo root.
              # `lefthook install` in the shellHook below wires them into
              # `.git/hooks/` so they fire on commit/push.
              pkgs.lefthook

              # LLVM lld linker — selected on Linux via .cargo/config.toml. Far
              # lower peak memory + faster than the default GNU bfd linker when
              # linking the many large debug test binaries (~95% debug info), so
              # `cargo test --workspace` no longer risks OOM at high parallelism.
              pkgs.lld

              # `cargo sqlx prepare` regenerates the committed `.sqlx/` offline
              # query cache that the repository `query!` macros check against in
              # CI (SQLX_OFFLINE=true).
              pkgs.sqlx-cli

              # `task` (go-task) runs the repo's many verification surfaces
              # (cargo, vitest, biome, buf, junius check) from one Taskfile.
              pkgs.go-task

              # Playwright browsers from nixpkgs. The npm-downloaded browsers
              # are prebuilt against an FHS dynamic loader and can't launch on
              # NixOS (they die at startup — "Target closed"), so the M17 E2E
              # suite needs the nixpkgs-built, patchelf'd browsers. The npm
              # `@playwright/test` version is pinned in package.json to match
              # this driver so the browser revisions line up.
              # See https://wiki.nixos.org/wiki/Playwright.
              pkgs.playwright-driver.browsers
            ];

            shellHook = ''
              # Wire `lefthook.yml` hooks into `.git/hooks/`. Idempotent
              # and quick — skipped outside a working tree (e.g. CI
              # building from a `git archive` tarball).
              if [ -d .git ]; then
                lefthook install >/dev/null
              fi

              # Point Playwright at the nixpkgs browsers — only on NixOS,
              # where the npm-downloaded browsers can't run. On Ubuntu CI
              # (no /etc/NIXOS) this stays unset so `playwright install`
              # keeps downloading FHS-compatible browsers as before.
              if [ -e /etc/NIXOS ]; then
                export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
                export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
              fi

              echo "── Junius dev shell ──"
              echo "  rustc : $(rustc --version)"
              echo "  node  : $(node --version)"
              echo "  pnpm  : $(pnpm --version)"
              echo "  biome : $(biome --version 2>&1 | head -n1)"
              echo "  buf   : $(buf --version)"
              echo "  protoc: $(protoc --version)"
            '';
          };
        });

      # M24 precompiled-image packages. Both Rust binaries are built for
      # `x86_64-unknown-linux-musl` with `+crt-static`, so the result is
      # fully self-contained and runs on a `FROM scratch` base. Invoked
      # from the docker builders via `nix build .#juniusd-headless-static`
      # / `.#juniusd-static` / `.#junius-static`.
      #
      # Outputs are gated to `x86_64-linux` because v1 ships amd64 only
      # (per the M24 doc — arm64 lands in M25). Building from a different
      # host system errors clearly; that's intentional.
      packages = forAllSystems (system:
        if system == "x86_64-linux" then
          let
            pkgs = pkgsFor system;
            # Musl cross-compile toolchain. Necessary because some deps
            # (`aws-lc-sys`, `zstd-sys`) compile C source via the `cc`
            # crate at build time; without a proper musl-targeting C
            # compiler their .o files reference glibc symbols
            # (`__isoc23_sscanf`, `__memcpy_chk`, …) that musl doesn't
            # provide, and the static link fails.
            pkgsMusl = pkgs.pkgsCross.musl64;
            muslCC = pkgsMusl.stdenv.cc;
            targetPrefix = muslCC.targetPrefix;

            rustToolchain = rustToolchainFor pkgs;
            craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

            # Source: the whole workspace, minus dirs cargo doesn't read.
            # Keeping proto/, migrations/, .sqlx/, build.rs scripts, *.po
            # — all of which cargo consumes at build time — falls out of
            # the permissive default.
            #
            # `dist/` is intentionally NOT excluded: the `full` image's
            # docker build runs `pnpm shell build` to produce
            # `platform/frontend/dist/` before invoking `nix build
            # .#juniusd-static`, and `rust-embed`'s build script reads
            # that dir to bake the SPA into the binary. For the headless
            # variant the dir is empty / absent and the filter difference
            # is a no-op.
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let base = baseNameOf (toString path); in
                base != "target"
                && base != "node_modules"
                && base != ".git"
                && base != ".github";
            };

            target = "x86_64-unknown-linux-musl";

            # Shared crane args. `+crt-static` is the musl default but we
            # set it explicitly so the audit trail is unambiguous. The
            # `CC_…` / `AR_…` / `CARGO_TARGET_…_LINKER` env vars route
            # both C-dep builds (`cc` crate) and the final link through
            # the musl cross-toolchain.
            commonArgs = {
              inherit src;
              strictDeps = true;
              CARGO_BUILD_TARGET = target;
              CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
              CC_x86_64_unknown_linux_musl = "${muslCC}/bin/${targetPrefix}cc";
              CXX_x86_64_unknown_linux_musl = "${muslCC}/bin/${targetPrefix}c++";
              AR_x86_64_unknown_linux_musl = "${muslCC.bintools.bintools}/bin/${targetPrefix}ar";
              CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslCC}/bin/${targetPrefix}cc";
              nativeBuildInputs = [
                pkgs.protobuf
                pkgs.pkg-config
                muslCC
              ];
              # No `buildInputs` — we want zero dynamic libs at runtime.
              # rustls handles TLS; everything else is pure Rust.
            };

            # Workspace deps compiled once and reused across binaries.
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;

            mkBin = { pname, cargoExtraArgs }:
              craneLib.buildPackage (commonArgs // {
                inherit cargoArtifacts pname cargoExtraArgs;
                version = "0.0.0";
                # CI runs tests separately; image-build is not the place.
                doCheck = false;
              });

            # Hermetic pnpm dep cache. Hash is auto-pinned to the
            # current `pnpm-lock.yaml`; nix rebuilds it whenever the
            # lockfile changes. First-time hash discovery: set
            # `hash = lib.fakeHash`, run a build, copy the suggested
            # hash from the nix error.
            pnpmDeps = pkgs.pnpm.fetchDeps {
              pname = "junius";
              src = ./.;
              # `fetcherVersion = 3` is the current nixpkgs-recommended
              # one (the 2 → 3 deprecation lands in 26.11).
              fetcherVersion = 3;
              hash = "sha256-gbALDqmSIHDcjkQYjYwbTlg5u3yEsyh9mEa58g8i2gA=";
            };

            # Built SPA assets, baked into the `full` juniusd binary via
            # `rust-embed`. Pure nix derivation: deps come from the
            # hermetic `pnpmDeps` cache, build runs in the nix sandbox.
            frontend-bundle = pkgs.stdenvNoCC.mkDerivation {
              pname = "junius-frontend";
              version = "0.0.0";
              src = ./.;
              nativeBuildInputs = [
                pkgs.nodejs_24
                pkgs.pnpm
                pkgs.pnpm.configHook
              ];
              inherit pnpmDeps;
              buildPhase = ''
                runHook preBuild
                pnpm --filter @junius/shell build
                runHook postBuild
              '';
              installPhase = ''
                runHook preInstall
                mkdir -p $out
                cp -r platform/frontend/dist $out/dist
                runHook postInstall
              '';
            };

            # SSR bundle for the M23 `frontend` image. Vite SSR-bundles
            # `src/server.ts` itself (via `vite.node.config.ts`,
            # `noExternal: true`) into `dist/node-server/server.js` — a
            # single self-contained file with React + TanStack + every
            # plugin frontend inlined. The runtime image then needs only
            # `node + dist/`; no `node_modules`, no source files, no
            # workspace symlinks. Pre-pivot this derivation shipped the
            # entire pnpm workspace (498 MB on disk → 2.7 GB image);
            # bundling drops that by ~10×.
            frontend-ssr-bundle = pkgs.stdenvNoCC.mkDerivation {
              pname = "junius-frontend-ssr";
              version = "0.0.0";
              src = ./.;
              nativeBuildInputs = [
                pkgs.nodejs_24
                pkgs.pnpm
                pkgs.pnpm.configHook
              ];
              inherit pnpmDeps;
              buildPhase = ''
                runHook preBuild
                pnpm --filter @junius/shell-ssr build
                runHook postBuild
              '';
              installPhase = ''
                runHook preInstall
                mkdir -p $out
                cp -r platform/frontend-ssr/dist $out/dist
                runHook postInstall
              '';
              # `dontFixup` skips the stdenv strip/patchelf pass — no ELF
              # binaries to fix; saves a few seconds on every build.
              dontFixup = true;
            };

            # `full` image: juniusd with the SPA embedded via
            # `rust-embed`. Staged with the nix-built FE bundle dropped
            # into `platform/frontend/dist/` so cargo's build script
            # picks it up.
            juniusd-static = (mkBin {
              pname = "juniusd";
              cargoExtraArgs = "--locked -p platform --features embed-frontend";
            }).overrideAttrs (old: {
              # Inject the FE bundle into the cargo source tree before
              # cargo's build script runs.
              postUnpack = ''
                cp -r ${frontend-bundle}/dist source/platform/frontend/dist
                chmod -R u+w source/platform/frontend/dist
              '';
            });

            # `backend` image: juniusd without the embedded SPA.
            juniusd-headless-static = mkBin {
              pname = "juniusd";
              cargoExtraArgs = "--locked -p platform";
            };

            # Trimmed in-container CLI: drops source-build commands
            # (Build/Cache/Plugin {Enable,Disable}) and dev-tree-only
            # commands (Sync/Dev/New/Rpc) so the image carries only
            # check/migrate/plugin {list,info}/i18n/provision.
            junius-static = mkBin {
              pname = "junius";
              cargoExtraArgs = "--locked -p junius --no-default-features";
            };
          in {
            inherit
              juniusd-static juniusd-headless-static junius-static
              frontend-bundle frontend-ssr-bundle pnpmDeps;
            # Default `nix build` → headless juniusd. Picked because it's
            # the fastest probe (no FE bundle prep required).
            default = juniusd-headless-static;
          }
        else { });
    };
}
