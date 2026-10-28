# Glossary

- **Reading view**: the default, rendered, read-only view of a file. Task checkboxes are clickable.
- **Source view**: the editable plain-text view with markdown syntax highlighting. Toggled with `Ctrl+E`; also shown for files with unresolved MDeer conflicts.
- **Tab**: one open file. The tab bar is only shown when more than one tab is open.
- **Window**: the single MDeer window owned by the primary instance (see ADR-0003).
- **Primary instance**: the first running `mdeer` process; later invocations forward their files to it.
- **Autosave**: writing the file to disk ~500 ms after typing stops, on focus loss and on close.
- **External change**: a change to an open file made outside MDeer.
- **Merge**: combining local edits and external changes relative to their common earlier version.
- **Conflict**: overlapping local edits and external changes that cannot be merged automatically. A conflict can be saved but still need resolution.
