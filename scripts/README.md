# scripts

## `bmad-auto.sh` — unattended BMad sprint orchestrator

Drives a BMad sprint end-to-end without operator interaction. For each backlog
story in the current in-progress epic, it loops through the standard BMad
skills (each in a fresh `claude -p` invocation, so context never grows across
stories) and lets the shell handle git, GitHub, and CI orchestration.

### Per-story flow

1. `bmad-create-story` — write the story spec, set status `backlog → ready-for-dev`
2. `git checkout -b story/<key>`, commit story spec
3. `bmad-dev-story` — implement, test, set status `ready-for-dev → review`
4. commit implementation
5. `bmad-code-review` — adversarial review, address must-fix findings inline
6. commit review fixes (if any)
7. `git push -u origin story/<key>`, `gh pr create --base main`
8. `gh pr checks --watch` — on red CI, fresh `claude -p` self-heals
   (bounded by `MAX_HEAL_ATTEMPTS`, default 3)
9. `gh pr merge --squash --delete-branch`
10. `git checkout main && git pull --ff-only`, repeat

### Epic boundary

When the in-progress epic has no remaining `backlog` stories, the script
triggers `bmad-retrospective` (in `-p` mode, producing a draft) and **stops**
so the operator can review the epic deliverables and refine the retro before
the next epic starts.

### Prerequisites

- `git`, `gh` (authenticated for the repo's host), `yq` (mikefarah), `claude` CLI
- A clean working tree on `main`
- A populated `_bmad-output/implementation-artifacts/sprint-status.yaml`
- BMad skills installed (`.claude/skills/bmad-*`)

### Usage

```sh
# from anywhere inside the repo
./scripts/bmad-auto.sh                # process all stories in the current epic, then stop at boundary
./scripts/bmad-auto.sh --once         # process exactly one story, then stop
./scripts/bmad-auto.sh --dry-run      # print actions, run no commands
./scripts/bmad-auto.sh --skip-review  # skip code-review step
./scripts/bmad-auto.sh --skip-retro   # skip retrospective at epic boundary
./scripts/bmad-auto.sh --help
```

### Config (env-overridable)

| Var                              | Default                                                | Purpose                                       |
| -------------------------------- | ------------------------------------------------------ | --------------------------------------------- |
| `BMAD_SPRINT_FILE`               | `_bmad-output/implementation-artifacts/sprint-status.yaml` | Sprint state                              |
| `BMAD_STORIES_DIR`               | `_bmad-output/implementation-artifacts`                | Where story files live                        |
| `BMAD_MAIN_BRANCH`               | `main`                                                 | Base branch for PRs                           |
| `BMAD_MAX_HEAL_ATTEMPTS`         | `3`                                                    | Self-heal retries before bailing              |
| `BMAD_CLAUDE_PERMISSION_MODE`    | `bypassPermissions`                                    | `--permission-mode` for each `claude -p`      |
| `BMAD_CLAUDE_MODEL`              | _(inherit)_                                            | Override `--model` (e.g. `opus`, `sonnet`)    |
| `BMAD_LOG_DIR`                   | `.bmad-auto-logs`                                      | Where per-invocation logs land (gitignored)   |
| `BMAD_CI_WATCH_INTERVAL`         | `30`                                                   | Seconds between CI status polls               |

### Exit codes

- `0` — sprint complete, or epic boundary reached (operator should inspect)
- `1` — preflight failure (missing tool, dirty worktree, missing files)
- `2` — bad CLI arg
- `3` — CI still red after `MAX_HEAL_ATTEMPTS` heal attempts
- non-zero from `claude` propagates as `1` (a failed skill aborts the run)

### Safety notes

- Runs `claude` with `--permission-mode bypassPermissions` by default. The
  script is meant for **unattended** use — only run it in a workspace you
  trust. To use it more conservatively, set `BMAD_CLAUDE_PERMISSION_MODE=acceptEdits`.
- Self-heal is bounded and explicitly forbidden from disabling CI checks,
  amending history, or force-pushing.
- The retrospective is interactive by design; `-p` mode produces a **draft**
  marked for human review. Treat it as a starting point, not a final document.
- The script refuses to start with a dirty worktree and refuses to reuse an
  existing `story/<key>` branch — clean up before re-running.
