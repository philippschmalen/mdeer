# Single instance, one window, files as tabs

The first `mdeer` process owns the only window. Later invocations (CLI, Explorer double-click) send absolute paths over an `interprocess` local socket (per-user name) and exit; the owner opens or focuses a tab and raises the window. If connecting fails, the new process becomes the owner. The tab bar is hidden while only one file is open, so the common case still looks like "one file, one window".

Rejected: one process per window (no tabs from Explorer) and Windows-only IPC (`WM_COPYDATA`), which isn't cross-platform.
