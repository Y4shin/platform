# Setting up your environment

The whole toolchain — Rust, Node 24, pnpm, biome, buf, `task`, `sqlx-cli` — comes
from the project's Nix flake. You don't install any of it by hand.

## Get a dev shell

```bash
# Enter the dev shell explicitly…
nix develop

# …or set up direnv once so the shell loads automatically on `cd`:
direnv allow
```

Everything in this book assumes you're inside that shell. If a command "isn't
found", you almost certainly aren't.

## Day-to-day commands

Work goes through `task` (run `task` alone to list everything; see
[`Taskfile.yml`](../../../../Taskfile.yml)). The ones you'll use constantly:

| Command | Does |
| --- | --- |
| `task dev` | Run the host (`juniusd`) + the Vite dev server |
| `task sync` | Regenerate composition glue from the deployment config |
| `task migrate` | Apply host + plugin migrations to the dev database |
| `task lint` | clippy (`-D warnings`), biome, buf lint, `junius check` |
| `task fmt` | Apply all formatters (`cargo fmt`, biome, `buf format`) |
| `task test` / `test:rust` / `test:js` / `test:e2e` | The test suites |
| `task ci` | The full local gate (fmt-check, lint, tests, buf, i18n, E2E) |

Under the hood these wrap the `junius` CLI against `dev/platform.toml`. You can
call the CLI directly too — `cargo run -p junius -- <cmd>` — which is handy when
you want a flag `task` doesn't expose. The [CLI reference](../reference/cli.md)
lists every subcommand.

## Start the backing services

Integration tests, the dev server and E2E tests need Docker. The local backing
stack (Postgres, Authentik, RabbitMQ, MinIO, mailpit, the LGTM observability
stack) comes up with one command:

```bash
task infra:up      # start the stack
task infra:down    # stop it again
```

With the stack up, `task dev` runs migrations and boots both the host and the
Vite dev server. Visit the printed URL and you have a working Junius with the
`events` and `admin` plugins enabled.

## A note on Docker and tests

- **Rust unit tests** (`task test:rust:unit`) need nothing — they're DB-less.
- **Rust integration tests**, **E2E tests** and the **dev server** need Docker.
- `task ci` *auto-skips* E2E when Docker is unreachable — but **CI always runs
  it**. Before pushing anything that touches end-to-end behaviour, run
  `task test:e2e` locally with the stack up.

## Verify your setup

```bash
# Inside the dev shell:
task                     # lists all tasks — confirms `task` is on PATH
cargo run -p junius -- plugin list --config dev/platform.toml
```

The second command should print the enabled plugins (`events`, `admin`). If it
does, you're ready to scaffold your own.
