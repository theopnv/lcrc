#!/usr/bin/env bash
# bmad-auto.sh — drive a BMad sprint unattended.
#
# For each backlog story in the current in-progress epic:
#   1. fresh `claude -p` invocation runs bmad-create-story
#   2. branch from main, commit story spec
#   3. fresh `claude -p` runs bmad-dev-story (implementation + tests)
#   4. commit implementation
#   5. fresh `claude -p` runs bmad-code-review
#   6. commit review fixes (if any)
#   7. push, open PR, watch CI
#   8. on red CI: fresh `claude -p` self-heals up to MAX_HEAL_ATTEMPTS times
#   9. on green CI: squash-merge, delete branch, sync main
#
# At the end of an epic (no more backlog stories under the in-progress epic),
# triggers bmad-retrospective and stops for operator inspection.
#
# See scripts/README.md for setup, flags, and config knobs.

set -euo pipefail

# ─────────────────────────────────────────────────────────────
# Config (env-overridable)
# ─────────────────────────────────────────────────────────────
SPRINT_FILE="${BMAD_SPRINT_FILE:-_bmad-output/implementation-artifacts/sprint-status.yaml}"
STORIES_DIR="${BMAD_STORIES_DIR:-_bmad-output/implementation-artifacts}"
MAIN_BRANCH="${BMAD_MAIN_BRANCH:-main}"
MAX_HEAL_ATTEMPTS="${BMAD_MAX_HEAL_ATTEMPTS:-3}"
# CLAUDE_PERMISSION_MODE="${BMAD_CLAUDE_PERMISSION_MODE:-bypassPermissions}"
CLAUDE_PERMISSION_MODE="${BMAD_CLAUDE_PERMISSION_MODE:-acceptEdits}"
CLAUDE_MODEL="${BMAD_CLAUDE_MODEL:-}"   # empty = inherit user default
LOG_DIR="${BMAD_LOG_DIR:-.bmad-auto-logs}"
CI_WATCH_INTERVAL="${BMAD_CI_WATCH_INTERVAL:-30}"

DRY_RUN=false
ONCE=false
SKIP_REVIEW=false
SKIP_RETRO=false

# ─────────────────────────────────────────────────────────────
# Args
# ─────────────────────────────────────────────────────────────
usage() {
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
  cat <<'EOF'

Flags:
  --dry-run        Print actions, do not invoke claude/git/gh.
  --once           Process at most one story, then stop.
  --skip-review    Skip the bmad-code-review step.
  --skip-retro     Skip the retrospective at epic boundary.
  -h, --help       Show this message.

Env overrides: BMAD_SPRINT_FILE, BMAD_STORIES_DIR, BMAD_MAIN_BRANCH,
  BMAD_MAX_HEAL_ATTEMPTS, BMAD_CLAUDE_PERMISSION_MODE, BMAD_CLAUDE_MODEL,
  BMAD_LOG_DIR, BMAD_CI_WATCH_INTERVAL.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)     DRY_RUN=true; shift ;;
    --once)        ONCE=true; shift ;;
    --skip-review) SKIP_REVIEW=true; shift ;;
    --skip-retro)  SKIP_RETRO=true; shift ;;
    -h|--help)     usage; exit 0 ;;
    *)             echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# ─────────────────────────────────────────────────────────────
# Logging
# ─────────────────────────────────────────────────────────────
log()  { printf '\033[1;36m[bmad-auto %(%H:%M:%S)T]\033[0m %s\n' -1 "$*"; }
warn() { printf '\033[1;33m[bmad-auto %(%H:%M:%S)T]\033[0m %s\n' -1 "$*" >&2; }
err()  { printf '\033[1;31m[bmad-auto %(%H:%M:%S)T] ERR\033[0m %s\n' -1 "$*" >&2; }
die()  { err "$*"; exit 1; }

# ─────────────────────────────────────────────────────────────
# Preflight
# ─────────────────────────────────────────────────────────────
require() {
  for cmd in "$@"; do
    command -v "$cmd" >/dev/null || die "missing required command: $cmd"
  done
}
require git gh yq claude

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || die "not in a git repo"
cd "$REPO_ROOT"

[[ -f "$SPRINT_FILE" ]] || die "sprint file not found: $SPRINT_FILE"
[[ -d "$STORIES_DIR" ]] || die "stories dir not found: $STORIES_DIR"

mkdir -p "$LOG_DIR"

# ─────────────────────────────────────────────────────────────
# Sprint state queries (yq)
# ─────────────────────────────────────────────────────────────
get_in_progress_epic() {
  yq e '.development_status | to_entries
    | map(select(.key | test("^epic-[0-9]+$")) | select(.value == "in-progress"))
    | .[0].key // ""' "$SPRINT_FILE" \
    | sed 's/^epic-//'
}

get_first_backlog_epic() {
  yq e '.development_status | to_entries
    | map(select(.key | test("^epic-[0-9]+$")) | select(.value == "backlog"))
    | .[0].key // ""' "$SPRINT_FILE" \
    | sed 's/^epic-//'
}

# Returns the first story key under epic $1 whose status is "backlog".
find_next_backlog_story() {
  local epic="$1"
  yq e ".development_status | to_entries
    | map(select(.key | test(\"^${epic}-[0-9]+-\")) | select(.value == \"backlog\"))
    | .[0].key // \"\"" "$SPRINT_FILE"
}

# ─────────────────────────────────────────────────────────────
# Claude invocation
# ─────────────────────────────────────────────────────────────
invoke_claude() {
  local label="$1" prompt="$2"
  local logfile="$LOG_DIR/$(date +%Y%m%d-%H%M%S)-${label}.log"
  log "→ claude: $label"
  log "  log: $logfile"
  if $DRY_RUN; then
    printf '  [dry-run] prompt:\n%s\n' "$prompt" | sed 's/^/    /'
    return 0
  fi
  local args=(--print --permission-mode "$CLAUDE_PERMISSION_MODE")
  [[ -n "$CLAUDE_MODEL" ]] && args+=(--model "$CLAUDE_MODEL")
  # Stream live to stdout AND capture to log. PIPESTATUS preserves claude's exit code.
  local rc=0
  claude "${args[@]}" "$prompt" 2>&1 | tee "$logfile" || rc=${PIPESTATUS[0]}
  if (( rc != 0 )); then
    err "claude failed (exit $rc). Tail of log:"
    tail -n 40 "$logfile" >&2 || true
    die "claude invocation failed: $label"
  fi
  log "← claude done: $label"
}

# ─────────────────────────────────────────────────────────────
# Git helpers
# ─────────────────────────────────────────────────────────────
sync_main() {
  log "Syncing $MAIN_BRANCH"
  if $DRY_RUN; then return 0; fi
  git checkout "$MAIN_BRANCH"
  git pull --ff-only
}

ensure_clean_worktree() {
  if ! git diff --quiet || ! git diff --cached --quiet; then
    die "worktree has uncommitted changes — commit/stash before running"
  fi
  if [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
    warn "untracked files present (will be carried into the story branch):"
    git ls-files --others --exclude-standard | sed 's/^/    /' >&2
  fi
}

commit_all_if_changes() {
  local msg="$1"
  if $DRY_RUN; then
    log "  [dry-run] would commit: $msg"
    return 0
  fi
  git add -A
  if git diff --cached --quiet; then
    log "  no changes to commit ($msg)"
    return 0
  fi
  git commit -m "$msg"
  log "  committed: $msg"
}

# ─────────────────────────────────────────────────────────────
# PR helpers
# ─────────────────────────────────────────────────────────────
extract_story_title() {
  local story_file="$1" story_key="$2"
  if [[ -f "$story_file" ]]; then
    local h1
    h1=$(grep -m1 '^# ' "$story_file" | sed 's/^# *//' || true)
    [[ -n "$h1" ]] && { echo "$h1"; return; }
  fi
  echo "Story $story_key"
}

watch_ci_with_heal() {
  local pr_url="$1" branch="$2" story_key="$3"
  local attempts=0
  while true; do
    log "Watching CI on $pr_url"
    if gh pr checks "$pr_url" --watch --interval "$CI_WATCH_INTERVAL"; then
      log "✓ CI green"
      return 0
    fi
    attempts=$((attempts + 1))
    if (( attempts > MAX_HEAL_ATTEMPTS )); then
      err "CI still failing after $MAX_HEAL_ATTEMPTS heal attempts."
      err "  PR: $pr_url"
      err "  Inspect failures, fix manually, then re-run."
      exit 3
    fi
    log "✗ CI red. Self-heal attempt ${attempts}/${MAX_HEAL_ATTEMPTS}"

    local fail_summary
    fail_summary=$(gh pr checks "$pr_url" 2>&1 || true)

    invoke_claude "self-heal-${story_key}-${attempts}" "CI is failing on PR ${pr_url} (branch ${branch}, story ${story_key}).

Check status:
${fail_summary}

Investigate the actual failure logs (use \`gh run list --branch ${branch} --limit 5\` and \`gh run view <run-id> --log-failed\` to fetch them), identify the root cause, fix the underlying problem, commit the fix to the current branch (${branch}), and push with \`git push\`.

Constraints:
- Do NOT bypass checks, edit CI workflow files to skip them, or use --no-verify.
- Do NOT amend or force-push; create a new commit.
- Stay on branch ${branch}; do not create new branches.
- Do not open new PRs; the existing PR (${pr_url}) is the target."

    # If claude committed but didn't push, push now.
    if ! $DRY_RUN; then
      git push 2>/dev/null || true
    fi
  done
}

# ─────────────────────────────────────────────────────────────
# Per-story flow
# ─────────────────────────────────────────────────────────────
process_story() {
  local epic="$1" story_key="$2"
  local branch="story/${story_key}"
  local story_file="${STORIES_DIR}/${story_key}.md"

  log "═══════════════════════════════════════════════════"
  log "Story: $story_key  (epic $epic)"
  log "Branch: $branch"
  log "═══════════════════════════════════════════════════"

  if ! $DRY_RUN; then
    if git show-ref --verify --quiet "refs/heads/$branch"; then
      die "branch $branch already exists locally — clean up before running"
    fi
    git checkout -b "$branch"
  fi

  # 1. Create story spec.
  invoke_claude "create-story-${story_key}" "Use the bmad-create-story skill to create the next backlog story.

Source of truth: ${SPRINT_FILE} (auto-discover the next story in document order — do not ask which one).

Run the workflow end-to-end without prompting for input:
- Pick the first story whose status is \"backlog\".
- Generate the comprehensive story file at ${STORIES_DIR}/{story_key}.md.
- Update the sprint-status entry from backlog → ready-for-dev.

Make reasonable assumptions if anything is ambiguous. Do not ask clarifying questions; this is an unattended run."

  commit_all_if_changes "story(${story_key}): create story spec"

  # 2. Implement.
  invoke_claude "dev-story-${story_key}" "Use the bmad-dev-story skill to implement story ${story_file}.

Run the workflow end-to-end:
- Implement every task and subtask in the story file.
- Write the tests called for in the story (red-green-refactor).
- Run the full test suite and linters; fix any regressions.
- Update the story file's task checkboxes, File List, Dev Agent Record, and Status (→ review) per the workflow.
- Update sprint-status.yaml: in-progress while working, then review when done.

This is unattended. Do not pause for review or schedule a 'next session'. Run until the story is complete or a real HALT condition triggers."

  commit_all_if_changes "feat(${story_key}): implement story"

  # 3. Code review.
  if ! $SKIP_REVIEW; then
    invoke_claude "code-review-${story_key}" "Use the bmad-code-review skill to review the changes on the current branch (${branch}) for story ${story_file}.

Compare against ${MAIN_BRANCH}. Triage findings into the standard categories (must-fix, should-fix, nits). For must-fix and clear should-fix items, apply the fixes inline (commit them; don't just file follow-ups). Update the story's review section per the workflow.

Unattended run — do not ask for confirmation."
    commit_all_if_changes "review(${story_key}): address review feedback"
  fi

  # 4. Push, PR, CI, merge.
  if $DRY_RUN; then
    log "  [dry-run] would: push, gh pr create, watch CI, gh pr merge --squash --delete-branch"
    return 0
  fi

  log "Pushing $branch"
  git push -u origin "$branch"

  local pr_title pr_body pr_url
  pr_title="$(extract_story_title "$story_file" "$story_key")"
  pr_body="Story \`${story_key}\` — automated PR.

Spec: \`${story_file}\`
Generated by \`scripts/bmad-auto.sh\`."

  log "Opening PR (base=$MAIN_BRANCH)"
  pr_url=$(gh pr create --base "$MAIN_BRANCH" --head "$branch" \
    --title "$pr_title" --body "$pr_body")
  log "PR: $pr_url"

  watch_ci_with_heal "$pr_url" "$branch" "$story_key"

  log "Merging (squash, delete branch)"
  gh pr merge "$pr_url" --squash --delete-branch --subject "$pr_title"

  log "✓ Story $story_key merged into $MAIN_BRANCH"
}

# ─────────────────────────────────────────────────────────────
# Epic boundary
# ─────────────────────────────────────────────────────────────
handle_epic_completion() {
  local epic="$1"
  log "═══════════════════════════════════════════════════"
  log "EPIC ${epic} COMPLETE"
  log "═══════════════════════════════════════════════════"

  if ! $SKIP_RETRO; then
    invoke_claude "retro-epic-${epic}" "Use the bmad-retrospective skill to run the retrospective for epic ${epic}.

The epic is complete (no remaining backlog stories under it). This is an unattended run, so:
- Where the workflow asks the user to confirm the epic number, confirm epic ${epic}.
- Where the workflow asks for the user's perspective in dialogue, synthesize a reasonable view from the story files, dev agent records, and review notes — and clearly mark those passages as \"draft (synthesized; awaiting human review)\".
- Save the retrospective document at the path the workflow prescribes.
- Update sprint-status.yaml to mark epic-${epic}-retrospective as done.

Do not block on missing user input."
  fi

  log "═══════════════════════════════════════════════════"
  log "Stopping for operator review."
  log "  • Inspect epic ${epic} deliverables on ${MAIN_BRANCH}."
  log "  • Review the retrospective draft (in ${STORIES_DIR}/) and refine."
  log "  • When ready, re-run \`scripts/bmad-auto.sh\` to start the next epic."
  log "═══════════════════════════════════════════════════"
}

# ─────────────────────────────────────────────────────────────
# Main loop
# ─────────────────────────────────────────────────────────────
main() {
  ensure_clean_worktree
  while true; do
    sync_main

    local epic
    epic=$(get_in_progress_epic)
    if [[ -z "$epic" ]]; then
      epic=$(get_first_backlog_epic)
      if [[ -z "$epic" ]]; then
        log "No remaining epics. Sprint complete!"
        exit 0
      fi
      log "No in-progress epic; will start epic-${epic}"
    fi

    local next_story
    next_story=$(find_next_backlog_story "$epic")
    if [[ -z "$next_story" ]]; then
      handle_epic_completion "$epic"
      exit 0
    fi

    process_story "$epic" "$next_story"

    if $ONCE; then
      log "--once: stopping after one story."
      exit 0
    fi
  done
}

main
