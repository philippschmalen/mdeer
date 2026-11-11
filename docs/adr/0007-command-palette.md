# ADR-0007: Ctrl+P command palette as the one command surface

**Date**: 2026-11-11
**Status**: accepted
**Deciders**: project maintainer, assisted by Claude

## Context

MDeer has no menus: every command is a keyboard shortcut listed in the README. Table editing adds about ten commands, too many to discover or remember from a README. The maintainer uses Obsidian, where `Ctrl+P` opens a command palette.

## Decision

Add an Obsidian-style command palette on `Ctrl+P`. It lists every command with its shortcut, filters by case-insensitive in-order letters, and hides commands that cannot run (table commands need Source view; row and column commands need the cursor in a table). It keeps no history. A single `Command` list owns each command's label and shortcut, and the shortcut dispatcher loops over that list, so keys and palette cannot drift apart.

## Alternatives Considered

### Right-click context menu

- **Pros**: discoverable with the mouse, cheap in egui.
- **Cons**: only reachable where the cursor is; does not cover app-level commands.
- **Why not**: the palette covers every command from the keyboard.

### Floating table toolbar

- **Pros**: one-click row and column operations.
- **Cons**: a third UI surface just for tables.
- **Why not**: palette plus shortcuts make the commands easy enough; revisit if the mouse is missed.

## Consequences

### Positive

- One place to find every command; shortcuts are learned from it.
- Table commands need no menu or toolbar.

### Negative

- First modal surface in MDeer; it must hand focus back to the editor on close.
- egui matches shortcuts ignoring extra Shift/Alt, so the command list must order longer chords first (enforced by a test).
