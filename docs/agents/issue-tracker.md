# Issue tracker: Beads (bd)

Issues and specs for this repo live in beads, a local Dolt database managed by the `bd` CLI. Run `bd prime` for the full command reference. Never use `bd edit` (it opens $EDITOR).

## Conventions

- **Create an issue**: `bd create --title="..." --description="..." --type=task|bug|feature|epic --priority=2`. Priority is 0-4 (0 = critical), not "high/low". Add labels with `-l a,b`.
- **Read an issue**: `bd show <id>` (`--json --include-comments` for machine-readable output).
- **List issues**: `bd list --status=open --json`, filtered with `--label`, `--label-any` or `--parent`. `bd ready` lists open, unblocked work.
- **Make an issue a child of a parent**: `bd create ... --parent=<parent-id>`.
- **Comment on an issue**: `bd comments add <id> "..."`
- **Apply / remove labels**: `bd label add <id> <label>` / `bd label remove <id> <label>`
- **Close**: `bd close <id> --reason="..."`

Specs live in the issue description (or `--design` for design decisions), not in separate markdown files.

## When a skill says "publish to the issue tracker"

Create a bd issue with `bd create`.

## When a skill says "fetch the relevant ticket"

Run `bd show <id>`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is an epic with **child** issues as tickets.

- **Map**: an epic labelled `wayfinder:map`, with the Notes / Decisions-so-far / Fog body in its description. `bd create --type=epic -l wayfinder:map ...`
- **Child ticket**: `bd create --parent=<map-id> -l wayfinder:<type> ...`, where `<type>` is `research`/`prototype`/`grilling`/`task`.
- **Blocking**: `bd dep add <child> <blocker>` (the child depends on the blocker). A ticket is unblocked when every blocker is closed.
- **Frontier query**: `bd ready --parent=<map-id>`, minus anything assigned. The first in creation order wins.
- **Claim**: `bd update <id> --claim`, the session's first write.
- **Resolve**: `bd comments add <id> "<answer>"`, then `bd close <id>`, then append a context pointer (gist + id) to the map's Decisions-so-far with `bd update <map-id> --description=...`.
