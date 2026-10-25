# Reading view by default, Ctrl+E toggles to Source view

A file opens in the read-only **Reading view**; `Ctrl+E` switches to a plain-text **Source view** with markdown syntax highlighting. We rejected Obsidian-style Live Preview (rendered inline, raw under the cursor) because it is very costly to build natively in egui, and a split pane because it halves the reading width. The only edit possible from Reading view is toggling task checkboxes.
