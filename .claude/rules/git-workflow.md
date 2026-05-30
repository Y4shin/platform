# Git workflow

## Branches

- Branch off `main`; **never commit directly to `main`**.
- Name issue branches `feature/<n>-<slug>` (e.g. `feature/2-shell-user-menu-logout`), matching
  the issue number. For non-issue work use a descriptive `<type>/<slug>`.
- Keep a branch scoped to **one issue / slice / logical change**. Rebase your own feature branch
  on `main` to stay current; don't force-push branches others are working on.

## Commits

- **Conventional Commits with a scope**, in English: `feat(ui): …`, `fix(auth): …`,
  `ci(buf): …`, `docs(prd): …`, `build(nix): …`, `test(events): …`, `refactor(sdk): …`.
- Small, logically self-contained commits. If you touch `pnpm-lock.yaml`, repin and commit the
  `flake.nix` `pnpmDeps` hash in the **same** change (see [nix.md](nix.md)).
- Commit/push only when the user asks.

## Before pushing a PR

Run the **full gate** and make sure it passes with **zero skips and zero warnings**:

```bash
task ci      # fmt-check, lint (clippy -D warnings), Rust+JS tests, buf, i18n, E2E
```

`task ci` auto-skips E2E when Docker is unreachable, **but CI always runs it** — start the dev
stack (`task infra:up`) and run `task test:e2e` locally before pushing anything that could affect
end-to-end behaviour. The pre-push `lefthook` hook also verifies flake FOD hashes.

## Opening the PR

- Base `main`; title = the issue/change title.
- Body lists the acceptance criteria you met and ends with `Closes #<n>` so the merge closes the
  issue (and unblocks its PRD dependency).
- All issue/PR text in English.

## A PR isn't done until CI is green

Passing `task ci` locally is necessary but not sufficient — the remote checks are the source of
truth. After the push you intend to be the final one, **wait for the GitHub Actions run triggered
by that commit to finish, and only treat the PR as done once those checks pass**:

```bash
gh pr checks --watch     # blocks until every check on the PR completes, then reports pass/fail
```

If a check fails, fix it, push again, and wait on the new run — repeat until the latest commit's
checks are green. Don't report a PR as complete (or hand it back to the user) on a pending or red run.

## CI mirror

CI ([.github/workflows/ci.yml](../../.github/workflows/ci.yml)) runs these jobs in parallel;
each maps to a local `task` target you can run to reproduce a failure:

| CI job | Local | Covers |
| --- | --- | --- |
| lint | `task ci:lint` | fmt-check, clippy, no-default `junius` build, biome, buf lint/format |
| check | `task ci:check` | workspace build, Rust+JS tests, buf breaking vs `main` |
| integration | `task ci:integration` | migrate the example deployment + verify the `.sqlx` cache |
| deployment | `task ci:deployment` | validate + build the example deployment |
| i18n | `task ci:i18n` | BE/FE catalog validation, extract-drift, pseudo completeness |
| e2e / e2e:split | `task ci:e2e` / `ci:e2e:split` | Playwright (embedded + SSR-split topologies) |

## Issue work is automated

For tracked slice issues, `/analyse-issue <n>` then `/implement-issue <n>` drive the branch →
strict TDD → PR flow end to end. Prefer them over doing the steps by hand.
