#!/usr/bin/env bash
# Forge helper for the feature-workflow skills (.claude/skills/).
#
# Detects which git host backs `origin` (GitHub → `gh`, Forgejo/Codeberg →
# `fgj`) and emits ONE targeted snippet for the requested key. The skills
# inject only the piece they need via dynamic context injection, e.g.
#
#     !`"$(git rev-parse --show-toplevel)/scripts/forge_detect.sh" cmd_create_pr`
#
# so a skill sees the exact, provider-correct command instead of the whole
# forge reference. This script is the single source of truth for the
# per-provider command shapes; docs/workflow/forge.md is the human narrative.
#
# Usage:
#   scripts/forge_detect.sh <key>
#   scripts/forge_detect.sh keys          # list every key
#
# Placeholders in emitted commands use <angle-brackets>; fill them in.

set -euo pipefail

key="${1:-}"
if [ -z "$key" ]; then
  echo "usage: scripts/forge_detect.sh <key>   (run 'scripts/forge_detect.sh keys' for the list)" >&2
  exit 64
fi

# --- detect provider + owner/repo from the remote ---------------------------
remote="$(git remote get-url origin 2>/dev/null || true)"
case "$remote" in
  *github.com*)                     FORGE=gh  ;;
  *codeberg.org*|*forgejo*|*gitea*) FORGE=fgj ;;
  *)
    # Emit a visible marker (stdout, so injection shows it) and stop.
    echo "UNKNOWN_FORGE: cannot tell provider from remote '$remote' — ask the user which CLI to use."
    exit 0
    ;;
esac
path="${remote%.git}"        # strip trailing .git
REPO="${path##*/}"           # basename                → repo
rest="${path%/*}"            # everything before it
OWNER="${rest##*[:/]}"       # last segment after / or : → owner

# pick <gh-text> / <fgj-text> by detected provider
pick() { if [ "$FORGE" = gh ]; then printf '%s\n' "$1"; else printf '%s\n' "$2"; fi; }

case "$key" in
  keys)
    cat <<'EOF'
git_type owner repo auth_check
cmd_get_issue cmd_create_issue cmd_list_issues cmd_comment cmd_close_issue
cmd_edit_labels cmd_create_pr ensure_labels cmd_attach_subissue ownership_note
EOF
    ;;

  git_type) printf '%s\n' "$FORGE" ;;
  owner)    printf '%s\n' "$OWNER" ;;
  repo)     printf '%s\n' "$REPO" ;;

  auth_check)      pick 'gh auth status' 'fgj auth status' ;;
  cmd_get_issue)   pick 'gh issue view <n> --json number,title,body,labels,state' \
                        'fgj issue view <n>' ;;
  cmd_create_issue) pick 'gh issue create --title "<t>" --body-file <f> --label <l> --milestone "<M>"' \
                         'fgj issue create --title "<t>" --body "<b>" --label <l>   # no --milestone: use a milestone:M<NN> label' ;;
  cmd_list_issues) pick 'gh issue list --label <l> --json number,title,state' \
                        'fgj issue list --label <l>' ;;
  cmd_comment)     pick 'gh issue comment <n> --body "<text>"' \
                        'fgj issue comment <n> --body "<text>"' ;;
  cmd_close_issue) pick 'gh issue close <n> --comment "<text>"' \
                        'fgj issue close <n>' ;;
  cmd_edit_labels) pick 'gh issue edit <n> --add-label <a> --remove-label <r>' \
                        'fgj issue edit <n> --label <a>' ;;
  cmd_create_pr)   pick 'gh pr create --base main --head <branch> --title "<t>" --body-file <f>' \
                        'fgj pr create --base main --head <branch> --title "<t>" --body "<b>"' ;;

  ensure_labels)
    # Idempotent label provisioning for the detected provider.
    labels=(
      "kind:feature|1d76db"     "kind:capability|0e8a16" "prd|5319e7"
      "mode:hitl|fbca04"        "mode:afk|c2e0c6"
      "status:todo|ededed"      "status:in-progress|0052cc"
      "status:needs-review|d93f0b" "status:done|0e8a16"
    )
    for spec in "${labels[@]}"; do
      name="${spec%%|*}"; color="${spec##*|}"
      if [ "$FORGE" = gh ]; then
        echo "gh label create '$name' --color $color --force"
      else
        echo "fgj label create '$name' --color '#$color' || true"
      fi
    done
    ;;

  cmd_attach_subissue)
    if [ "$FORGE" = gh ]; then
      cat <<EOF
# GitHub native sub-issue (true ownership). child id != issue number — resolve it first.
child_id=\$(gh api "repos/$OWNER/$REPO/issues/<child#>" --jq .id)
gh api --method POST "repos/$OWNER/$REPO/issues/<prd#>/sub_issues" -F sub_issue_id="\$child_id"
EOF
    else
      cat <<'EOF'
# Forgejo has no ownership hierarchy. Model it by convention:
#  - add "- [ ] #<child#> <title>" to the PRD issue's task list (fgj issue edit <prd#> --body ...)
#  - put a "Part of #<prd#>" line in the child issue body
# (Optional native dependency via raw REST; blocks-semantics, not ownership.)
EOF
    fi
    ;;

  ownership_note)
    pick 'GitHub: native sub-issues give the PRD issue true parent/child ownership + roll-up progress (wired via gh api).' \
         'Forgejo: NO ownership hierarchy — best-effort via the PRD issue task list + "Part of #<prd>" references only.' ;;

  *)
    echo "ERROR: unknown key '$key' — run 'scripts/forge_detect.sh keys'" >&2
    exit 65
    ;;
esac
