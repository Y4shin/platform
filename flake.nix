{
  description = "Junius — plugin-driven monolith for political work";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
    in {
      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          # Pin the Rust toolchain via rust-toolchain.toml — single source of truth.
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in {
          default = pkgs.mkShell {
            packages = [
              rustToolchain

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
            ];

            shellHook = ''
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
    };
}
