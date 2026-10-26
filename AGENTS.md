# MDeer - Lightweight markdown reader and editor

> Vision: Like notepad just for markdown

## Objective goals

- app startup time <= 1 second
- <50 ms reaction time

## Features

- blazingly fast, beautifully simple, lightweight
- one window, files as tabs (tab bar hidden for a single file), no project or workspace management
	- single instance: opening another file (CLI or Explorer) adds a tab to the running window
	- autosave changes
	- when closed, do not remember opened files
	- assets: render images, resolve relative links
- tokyo night theme
- familiar shortcuts like obsidian
- default view is rendered formatting like Obsidian
- written in Rust
- Linux-native (Ubuntu 24.04+); Windows 11 via WSL2/WSLg, see `docs/adr/0004-linux-only-windows-via-wsl.md`
- `mdeer --register` integrates with the Linux desktop and, inside WSL, with Windows Explorer's "Open with"
- cli
	- `mdeer <file>` opens a single file
	- `mdeer` opens all `.md` files in the directory as tabs


## Dev workflow

- trunk-based git workflow
- create short-lived branches for feature work
	- use conventional commit messages
	- commit early and often
	- push your changes to remote
	- merge to main
- agents work in the beads **team-maintainer** profile: commit, push, merge to main, `bd dolt push` and release without asking (a current "don't" instruction still wins)
- releases: bump `version` in `Cargo.toml` (semver), merge to main, then tag `vX.Y.Z` and push the tag; `release.yml` builds the Linux binary `mdeer-x86_64-linux` and publishes the GitHub Release. No permission needed to tag or push.
- prefer git worktrees to work in parallel
- use determinist quality-gates where reasonable
	- add reasonable pre-commit hooks based on modern Rust standards, tooling and conventions
	- add a postToolUse agent hook to automatically format, lint, type-check after editing a file
	- when using `git commit -m '<message>'`: add a preToolUse hook that removes any co-authorship messages like "Co-authored-by: ..."
	

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:46cd31e7 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

## Agent skills

### Issue tracker

Issues are tracked in beads (`bd`), a local Dolt database. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) applied as bd labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
