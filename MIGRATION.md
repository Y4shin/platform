# Migration: local `.claude/skills/` → `prd-workflow` plugin

Plan of record for replacing this repo's locally-installed skills with the shareable
**`prd-workflow`** plugin, so the same PRD/epic/slice workflow can be reused across repos
instead of copy-pasted into each one. Nothing has been migrated yet.

## What the plugin is

The plugin lives in a marketplace repo published at `https://codeberg.org/Yashin/skills.git`:

- `.claude-plugin/marketplace.json` → marketplace **`platform-workflows`**, exposing one
  plugin **`prd-workflow`** (v0.2.0).
- `plugins/prd-workflow/` contains:
  - `skills/` — **11 skills, full parity** with our local set (analyse-issue, create-epic,
    epic-to-prds, create-feature-prd, create-capability-prd, feature-prd-to-issues,
    capability-prd-to-issues, implement-issue, finalize-prd, finalize-epic, grill-me).
    Nothing is missing or stubbed.
  - `references/artifacts.md` — the shared three-tier / lifecycle reference.
  - `scripts/forge_detect.sh` — GitHub/Forgejo abstraction (byte-identical to our copy).
  - `scripts/prd_tool.pyz` — a bundled Python zipapp (built from `src/prd_tool/` via the
    flake + `uv run prd-tool-build`).

## What's improved vs. our in-repo `.claude/skills`

| Area | Local skills (now) | Plugin (`prd-workflow`) |
|---|---|---|
| **Distribution** | Copied into `.claude/skills/` per repo | Installable plugin + marketplace; shareable across repos; skills namespaced `/prd-workflow:analyse-issue` |
| **Machinery location** | Skills call `$(git rev-parse --show-toplevel)/scripts/forge_detect.sh` and `cat docs/workflow/artifacts.md` — must exist in *every* consuming repo | Calls `${CLAUDE_PLUGIN_ROOT}/scripts/...` and `${CLAUDE_PLUGIN_ROOT}/references/artifacts.md` — ships *with the plugin*, nothing duplicated into the repo |
| **State handling** | Prose-driven frontmatter reads/edits | New `prd_tool.pyz`: deterministic `resolve / assert-kind / set-slices / slices / prd-finalizable / epic prds·tick·finalizable / list`, declared via `allowed-tools` — makes the lifecycle gates programmatic |
| **Context** | Static | Dynamic injection via `` !`…` `` — live forge type, the artifacts reference, and a current `prd_tool list` inventory injected at load |
| **Versioning** | None | Versioned (0.2.0), reproducibly buildable (flake/uv/pyproject) |

Net: same workflow, but **portable and self-contained**, with a real helper tool replacing
hand-waved frontmatter edits.

## Migration steps

1. **Fix the prerequisite first — `python3`.** This is the one real blocker. `prd_tool.pyz`
   (and the `` !`…` `` injections that call it) need `python3` on PATH. Today on the base
   shell `python3` is **missing** — it only exists inside `nix develop` (`git`/`gh`/`task`
   are on PATH; `python3`/`uv` are not). Add `python3` to [flake.nix](flake.nix)'s devshell
   (or otherwise guarantee Claude Code launches with it), then sanity-check it parses the
   live tree:

   ```
   python3 .../prd_tool.pyz list   # should list m19/m20/m21/m22/m25/composition-contracts + epics
   ```

2. **Remove the now-duplicated local copies.**
   - Delete [.claude/skills/](.claude/skills/) (all 11) — otherwise `/analyse-issue` and
     `/prd-workflow:analyse-issue` both exist and compete.
   - The plugin bundles its own `forge_detect.sh` + `artifacts.md`, so these become orphaned:
     [scripts/forge_detect.sh](scripts/forge_detect.sh) and
     [docs/workflow/artifacts.md](docs/workflow/artifacts.md). **Check before deleting** —
     also tracked are [scripts/migrate_tracker_native.sh](scripts/migrate_tracker_native.sh)
     and [docs/workflow/forge.md](docs/workflow/forge.md), which may be one-off/docs the
     skills don't reference; confirm no other consumer.

3. **Install the plugin** (HTTPS remote):

   ```
   /plugin marketplace add https://codeberg.org/Yashin/skills.git
   /plugin install prd-workflow@platform-workflows
   ```

   To share with the team via the repo (rather than just one machine), commit
   `extraKnownMarketplaces` + `enabledPlugins` into a new `.claude/settings.json` — there's
   none today, so nothing conflicts.

4. **Verify against the live `docs/prd/` tree.** The plugin operates on exactly our layout
   (`docs/prd/<slug>/prd.md`, `slices/<n>-<slug>.md`, `docs/prd/epics/...`), and our existing
   artifacts (e.g. `composition-contracts`, `m19-core-platform-ui`) match the schema. Do one
   read-only `/prd-workflow:analyse-issue` dry run to confirm the namespaced skills resolve.

5. **Update [CLAUDE.md](CLAUDE.md).** The "Planning workflow" section invokes
   `/create-feature-prd`, `/analyse-issue`, … and points at `.claude/skills/*/SKILL.md`. After
   migration those become `/prd-workflow:*` and the SKILL files live in the plugin — re-point
   the wording.

## Two things to decide (not blockers)

- **`artifacts.md` has drifted.** Our [docs/workflow/artifacts.md](docs/workflow/artifacts.md)
  (8.9 KB) is **newer/larger** than the plugin's bundled `references/artifacts.md` (7.1 KB) —
  `forge_detect.sh` is identical, but the reference is not. After migration the plugin's copy
  wins, so upstream our local edits into the plugin repo first if we want to keep them.
- **The "shared" plugin is still Junius-flavored.** `implement-issue` hardcodes `task ci`,
  `junius-sdk`, `clippy.toml`, `unsafe_code = "forbid"`, `cargo sqlx`;
  `create-capability-prd`/`capability-prd-to-issues` reference `crates/junius-sdk`;
  `analyse-issue` references `task test:*`. Fine for this repo (it *is* Junius) and for sibling
  Junius repos. For genuinely different repos those gate/convention lines should be read from
  each repo's `CLAUDE.md` instead of baked into the skill — a follow-up for the plugin, not
  this migration.

## Status

In progress (uncommitted working tree):

- **Step 1 — done.** `python3` 3.15.0a7 is on PATH (nix-profile).
- **Step 2 — done.** Removed `.claude/skills/` (all 11), `scripts/forge_detect.sh`,
  `docs/workflow/artifacts.md`, and the now-orphaned forge cluster
  (`scripts/migrate_tracker_native.sh`, `docs/workflow/forge.md`). Re-pointed the dead links in
  `docs/prd/README.md` at the plugin's bundled `references/artifacts.md` + `scripts/forge_detect.sh`.
- **Step 3 — done.** Marketplace `platform-workflows` is registered and `prd-workflow@0.2.0` is
  installed (cached at `~/.claude/plugins/cache/platform-workflows/prd-workflow/0.2.0`, 11/11
  skills present); the `/prd-workflow:*` skills resolve. The committed config lives in
  **`.claude/settings.json`** (new file): `extraKnownMarketplaces` points at the codeberg HTTPS
  remote `https://codeberg.org/Yashin/skills.git` and `enabledPlugins` enables
  `prd-workflow@platform-workflows` — so a fresh clone gets the plugin without manual `/plugin`
  steps. (The local install resolved the marketplace to the directory `/home/patric/Projects/skills`
  for this machine; the committed file deliberately uses the shareable HTTPS source instead.)
- **Step 4 — done.** `prd_tool.pyz`, run from the **installed** path against the live `docs/prd/`
  tree, lists all 4 epics + 6 PRDs (`rc=0`); `forge_detect.sh git_type` → `github` (`rc=0`).
- **Step 5 — done.** `CLAUDE.md` and `docs/prd/README.md` now reference the plugin and its
  namespaced commands instead of `.claude/skills/`.
