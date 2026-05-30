#!/usr/bin/env bash
# One-time migration: old PRD-tracking model -> flat native tracker.
#
#   OLD: a `prd`-labelled issue OWNS its slices via native sub-issues (+ task list);
#        ordering is `## Blocked by` prose.
#   NEW: an EPIC is the only sub-issue parent (of its PRD + slice issues, flat); every other
#        relationship is a native issue dependency (PRD blocked_by slice, slice blocked_by
#        slice, PRD blocked_by PRD). See docs/workflow/forge.md.
#
# What it does (idempotent):
#   A. For each PRD, convert each PRD->slice SUB-ISSUE into a `PRD blocked_by slice` DEPENDENCY
#      and detach the slice from the PRD. Add slice->slice deps parsed from issue `## Blocked by`.
#   B. Create the thematic EPIC issues, attach each milestone's PRD + slices as native
#      sub-issues of its epic, scaffold docs/prd/epics/<epic-slug>/epic.md, and stamp `epic:` into the
#      grouped PRDs' frontmatter.
#   Closed PRDs are GROUP-ONLY (attached under their epic for record); dependency wiring is
#   skipped for them (moot on closed work).
#
# Usage:
#   scripts/migrate_tracker_native.sh            # dry-run: print the plan, mutate nothing
#   scripts/migrate_tracker_native.sh --apply    # perform the migration
#
# Safe to re-run: every write is guarded by a read-only check first.

set -euo pipefail

APPLY=0
[ "${1:-}" = "--apply" ] && APPLY=1

ROOT="$(git rev-parse --show-toplevel)"
FORGE="$("$ROOT/scripts/forge_detect.sh" git_type)"
OWNER="$("$ROOT/scripts/forge_detect.sh" owner)"
REPO="$("$ROOT/scripts/forge_detect.sh" repo)"
API_VER="2026-03-10"

if [ "$FORGE" != gh ]; then
  echo "This migration targets GitHub native sub-issues + dependencies; detected '$FORGE'." >&2
  echo "Port the gh api calls to your forge before running." >&2
  exit 1
fi

# The migration creates `epic`-labelled issues — make sure the label scheme (incl. `epic`)
# exists first. ensure_labels is idempotent (`--force` upserts).
if [ "$APPLY" = 1 ]; then
  eval "$("$ROOT/scripts/forge_detect.sh" ensure_labels)" >/dev/null
fi

# ---- thematic epic grouping (THE judgment call — review/edit before --apply) ---------------
# Format: "<epic-slug>|<title>|<space-separated PRD issue numbers>"
EPICS=(
  "platform-ux-foundations|Platform UX foundations|1 10"
  "developer-experience|Developer experience|15"
  "build-and-infra|Build & infra|21 26"
)

# ---- helpers -------------------------------------------------------------------------------
say()  { printf '%s\n' "$*"; }
mark() { if [ "$APPLY" = 1 ]; then printf '  + %s\n' "$*"; else printf '  DRY %s\n' "$*"; fi; }
skip() { printf '  = %s\n' "$*"; }

idof()        { gh api "repos/$OWNER/$REPO/issues/$1" --jq .id; }
state_of()    { gh api "repos/$OWNER/$REPO/issues/$1" --jq .state; }   # REST returns lowercase
subissues()   { gh api "repos/$OWNER/$REPO/issues/$1/sub_issues" --jq '.[].number' 2>/dev/null || true; }
blocked_by()  { gh api "repos/$OWNER/$REPO/issues/$1/dependencies/blocked_by" --jq '.[].number' 2>/dev/null || true; }

# A PRD's slices = its current sub-issue children UNION its existing blocked_by deps. The union
# makes Phase B correct both in dry-run (slices still parented by the PRD) and after --apply
# (slices already moved to deps). Deduped, numeric.
slices_of_prd() { { subissues "$1"; blocked_by "$1"; } | sort -un; }

# parse `#<n>` references under the issue's "## Blocked by" heading
blockers_in_body() {
  gh api "repos/$OWNER/$REPO/issues/$1" --jq .body 2>/dev/null \
    | awk 'tolower($0) ~ /^#+[[:space:]]*blocked by/ {f=1; next} /^#/ && f {f=0} f' \
    | grep -oE '#[0-9]+' | tr -d '#' | sort -un || true
}

add_dep() { # $1 blocked_by $2
  local n="$1" b="$2"
  [ "$n" = "$b" ] && return 0
  if blocked_by "$n" | grep -qx "$b"; then skip "#$n already blocked_by #$b"; return 0; fi
  if [ "$APPLY" = 1 ]; then
    gh api --method POST "repos/$OWNER/$REPO/issues/$n/dependencies/blocked_by" \
      -H "X-GitHub-Api-Version: $API_VER" -F issue_id="$(idof "$b")" >/dev/null
  fi
  mark "#$n blocked_by #$b"
}

attach_sub() { # attach child $2 under epic $1
  local e="$1" c="$2"
  if subissues "$e" | grep -qx "$c"; then skip "#$c already sub-issue of epic #$e"; return 0; fi
  if [ "$APPLY" = 1 ]; then
    gh api --method POST "repos/$OWNER/$REPO/issues/$e/sub_issues" \
      -F sub_issue_id="$(idof "$c")" >/dev/null
  fi
  mark "#$c -> sub-issue of epic #$e"
}

detach_sub() { # detach child $2 from parent $1
  local p="$1" c="$2"
  if ! subissues "$p" | grep -qx "$c"; then skip "#$c not a sub-issue of #$p (already detached)"; return 0; fi
  if [ "$APPLY" = 1 ]; then
    gh api --method DELETE "repos/$OWNER/$REPO/issues/$p/sub_issue" \
      -F sub_issue_id="$(idof "$c")" >/dev/null
  fi
  mark "#$c detached from #$p"
}

prd_path_of() { grep -rl "^prd_issue: $1$" "$ROOT"/docs/prd/*/prd.md 2>/dev/null | head -1; }

stamp_epic_field() { # $1 prd-issue#, $2 epic-slug
  local path; path="$(prd_path_of "$1")"
  [ -z "$path" ] && { say "  (no prd.md found for PRD #$1 — skip epic: stamp)"; return 0; }
  if grep -q "^epic: $2$" "$path"; then skip "$path already has epic: $2"; return 0; fi
  if [ "$APPLY" = 1 ]; then
    # insert `epic: <slug>` right after the `slug:` frontmatter line
    awk -v slug="$2" '1; /^slug:/ && !done {print "epic: " slug; done=1}' "$path" >"$path.tmp" \
      && mv "$path.tmp" "$path"
  fi
  mark "stamp epic: $2 into $path"
}

scaffold_epic_md() { # $1 epic-slug, $2 title, $3 epic-issue#, $4 "prd-slugs..."
  local slug="$1" title="$2" issue="$3" dir="$ROOT/docs/prd/epics/$1"
  if [ -f "$dir/epic.md" ]; then skip "$dir/epic.md already exists"; return 0; fi
  if [ "$APPLY" = 1 ]; then
    mkdir -p "$dir"
    {
      printf '%s\n' "---" "kind: epic" "title: $title" "slug: $slug" "epic_issue: $issue" \
        "prds:" "status: in-progress" "---" "" "# $title" "" \
        "> Migrated from milestone PRDs. Children tracked as native sub-issues of epic #$issue;" \
        "> ordering via native dependencies. Fill in the sections below if this epic stays active." \
        "" "## Problem / outcome" "## Constituent plugins & surfaces" "## Decomposition"
    } >"$dir/epic.md"
  fi
  mark "scaffold $dir/epic.md"
}

# ---- Phase A: PRD sub-issues -> dependencies ----------------------------------------------
say ""
say "== Phase A — convert PRD->slice sub-issues into dependencies =="
for prd in $(gh issue list --label prd --state all --json number --jq '.[].number'); do
  st="$(state_of "$prd")"
  say "PRD #$prd ($st):"
  if [ "$st" != open ]; then say "  (closed — group-only, skipping dependency wiring)"; continue; fi
  children="$(subissues "$prd")"
  [ -z "$children" ] && say "  (no sub-issue children)"
  for c in $children; do
    add_dep "$prd" "$c"      # PRD blocked_by slice (membership + close-gating)
    detach_sub "$prd" "$c"   # PRD stops parenting the slice
  done
  for c in $children; do
    for b in $(blockers_in_body "$c"); do add_dep "$c" "$b"; done   # slice ordering
  done
done

# ---- Phase B: create epics, attach children, scaffold docs --------------------------------
say ""
say "== Phase B — create epics + attach milestones =="
for spec in "${EPICS[@]}"; do
  slug="${spec%%|*}"; rest="${spec#*|}"; title="${rest%%|*}"; prds="${rest##*|}"
  say "Epic '$title' ($slug)  <-  PRDs: $prds"

  # find existing epic by exact title, else create
  epic_n="$(gh issue list --label epic --state all --json number,title \
              --jq ".[] | select(.title==\"$title\") | .number" | head -1)"
  if [ -n "$epic_n" ]; then
    skip "epic issue #$epic_n exists"
  elif [ "$APPLY" = 1 ]; then
    url="$(gh issue create --title "$title" --label epic \
            --body "Epic: $title (migrated). Children tracked as native sub-issues; ordering via dependencies.")"
    epic_n="${url##*/}"; mark "created epic issue #$epic_n"
  else
    epic_n="NEW"; mark "create epic issue '$title' (label epic)"
  fi

  for prd in $prds; do
    if [ "$epic_n" = NEW ]; then mark "#$prd -> sub-issue of epic '$title'"; else attach_sub "$epic_n" "$prd"; fi
    for c in $(slices_of_prd "$prd"); do
      if [ "$epic_n" = NEW ]; then mark "#$c -> sub-issue of epic '$title'"; else attach_sub "$epic_n" "$c"; fi
    done
    stamp_epic_field "$prd" "$slug"
  done

  scaffold_epic_md "$slug" "$title" "$epic_n" "$prds"
done

say ""
if [ "$APPLY" = 1 ]; then
  say "Done. Review the docs/prd/ changes and commit them alongside this migration."
else
  say "Dry run complete — no changes made. Re-run with --apply to perform the migration."
  say "Review the EPICS grouping at the top of this script first."
fi
