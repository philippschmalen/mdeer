# ADR-0006: Automatically merge external edits and persist conflict markers

**Date**: 2026-10-28
**Status**: accepted
**Deciders**: project maintainer, assisted by Copilot

## Context

Autosave stopped on external-change conflicts, while closing could discard the only in-memory edits. The maintainer wants automatic reconciliation instead of modal version-selection prompts. MDeer does not restore sessions and should retain a small native editing implementation.

## Decision

Keep merge, save, and close decisions in the per-file lifecycle module. Use diffy for three-way text merging; autosave labelled Git-style conflict markers into the original file, with per-region resolution controls in Source view and no extra recovery copies. A saved conflict may close normally; a failed save retains the Tab, and a failed Window close retains all Tabs unless the user explicitly confirms a discard.

## Alternatives Considered

### Modal version choice

- **Pros**: small implementation and explicit overwrite decisions.
- **Cons**: interrupts editing and misses opportunities for automatic clean merges.
- **Why not**: the maintainer prefers automatic merging.

### Append conflicts without markers

- **Pros**: preserves both text fragments without prompting.
- **Cons**: silently creates ambiguous content or broken Markdown.
- **Why not**: labelled regions keep unresolved decisions visible.

### Retained recovery copies

- **Pros**: preserves complete pre-merge versions.
- **Cons**: creates extra files and retention obligations.
- **Why not**: the maintainer explicitly selected merged marker text only.

### A custom merge algorithm or Git dependency

- **Pros**: a custom algorithm avoids a dependency; Git already supports merging.
- **Cons**: custom merging adds a correctness burden; Git or libgit2 adds unnecessary installation or binary weight.
- **Why not**: the focused diffy crate supplies in-process text merging without either cost.

## Consequences

### Positive

- One lifecycle interface concentrates merge, save, and close decisions.
- Clean external edits combine without intervention.
- Unresolved conflicts survive reopening without remembered sessions.

### Negative

- Other markdown readers also see literal conflict markers.
- Retaining a common earlier version increases per-file memory use.
- Further merging pauses while dirty text contains unresolved conflicts.
- Earlier successful writes are not rolled back if another Tab fails during Window close.

### Risks

- Text merging does not guarantee semantic correctness.
- Independent original versions are not retained, by explicit choice.
- Non-cooperating writers can replace a file after our checks; this is not crash recovery or a cross-program atomic compare-and-swap.
- Keep pending text on failure, recheck detected external changes before replacement, and cover merge/save/close scenarios through the same interface as callers.
