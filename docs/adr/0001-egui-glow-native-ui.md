# Native egui + glow UI instead of a webview

MDeer renders with egui (eframe, `glow` OpenGL backend) and `egui_commonmark`, not Tauri/wry. Startup ≤ 1 s and a small binary matter more than HTML-grade rendering; glow over wgpu saves several MB and cold-start time.

Consequence: markdown rendering is limited to what egui_commonmark draws, and Obsidian-style Live Preview (ADR-0002) is effectively off the table.
