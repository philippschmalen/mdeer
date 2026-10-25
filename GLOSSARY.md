# Glossary

- **Reading view**: the default, rendered, read-only view of a file. Task checkboxes are clickable.
- **Source view**: the editable plain-text view with markdown syntax highlighting. Toggled with `Ctrl+E`.
- **Tab**: one open file. The tab bar is only shown when more than one tab is open.
- **Window**: the single MDeer window owned by the primary instance (see ADR-0003).
- **Primary instance**: the first running `mdeer` process; later invocations forward their files to it.
- **Autosave**: writing the file to disk ~500 ms after typing stops, on focus loss and on close.
