# ADR-0008: Table editing as plain-text transforms in Source view

**Date**: 2026-11-11
**Status**: accepted
**Deciders**: project maintainer, assisted by Claude

## Context

Hand-writing GFM tables is tedious; changing their structure (adding, removing, moving rows and columns) is worst. Obsidian and Zettlr offer grid editors over rendered tables. ADR-0002 rejected live preview, and Reading view tables are drawn read-only by egui_commonmark.

## Decision

Table help lives in Source view only and is a set of pure text transforms on the one document buffer (`src/table.rs`): Tab/Shift+Tab/Enter move between cells, shortcuts and palette commands insert, delete and move rows and columns, a palette command inserts a sized skeleton, and a pasted spreadsheet range becomes a table. Only the table being acted on is reformatted (pipes aligned by Unicode display width, missing cells filled); untouched tables stay byte-for-byte. Every action is one undo step. Files stay plain GFM.

## Alternatives Considered

### Editable grid in Reading view or an overlay

- **Pros**: closest to a spreadsheet.
- **Cons**: needs a forked table renderer or a cell model kept in sync with the text. Obsidian 1.14/1.15's table editor and Zettlr's first table editor lost data when that model drifted from the source.
- **Why not**: keeping the text as the only truth removes that failure class; live preview is already rejected (ADR-0002).

### Reformat every table on save

- **Pros**: always tidy.
- **Cons**: large diffs in files the user did not mean to change.
- **Why not**: only tables the user edits are rewritten.

## Consequences

### Positive

- Small, unit-testable module; no new widget.
- Works with any GFM table, including ones written elsewhere.

### Negative

- Tables are still edited as text; no mouse handles.
- Tables nested in list items or block quotes get no help.
