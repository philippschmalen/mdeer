<p align="center"><img src="assets/logo.svg" width="128" alt="MDeer logo: a deer whose antlers form the letter M"></p>

# MDeer

A lightweight markdown reader and editor, like Notepad but for markdown. Written in Rust, Tokyo Night themed, starts in well under a second. Runs natively on Ubuntu and Windows 11.

- Opens files rendered (reading view); `Ctrl+E` switches to a syntax-highlighted source view
- Autosaves as you type; no projects, workspaces or remembered sessions
- One window, files as tabs; opening another file adds a tab to the running window
- Local images, relative links between notes, clickable task checkboxes

## Quickstart

**Windows 11:**

```powershell
winget install PhilippSchmalen.MDeer
mdeer --register
```

`--register` adds MDeer to Explorer's **Open with** for `.md` files, and makes it the default if no other app is set. No admin rights are needed. The exe is unsigned: a copy downloaded in a browser gets SmartScreen's "Unknown publisher" warning, so install it with winget.

**Ubuntu 24.04+:**

```sh
curl -fsSL https://raw.githubusercontent.com/philippschmalen/mdeer/main/install.sh | sh
```

This installs `mdeer` to `~/.local/bin` and makes it the default app for markdown. No sudo is needed. Then run `mdeer notes.md`.

**WSL2 (the older Windows route):** run the same installer inside an Ubuntu 24.04+ WSL2 distro. It also adds MDeer to Explorer's **Open with** menu for `.md` files. To make it the default, right-click a `.md` file → **Open with** → **Choose another app** → **MDeer** → **Always**. Files on `C:\` and inside WSL (`\\wsl.localhost\...`) both work. MDeer runs inside WSL and shows its window through WSLg; Windows only starts `wsl.exe`, so no unsigned `.exe` is involved.

## Usage

```sh
mdeer notes.md        # open a file
mdeer a.md b.md       # open several as tabs
mdeer                 # open every .md in the current folder as tabs
mdeer --register      # make MDeer the default app for .md (Linux desktop entry / Windows Explorer)
mdeer --unregister    # undo --register
mdeer --version       # print the version
```

| Shortcut | Action |
|---|---|
| `Ctrl+E` | Toggle reading / source view |
| `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | Bold / italic / link (source view) |
| `Enter` / `Shift+Enter` | Continue list or quote (Enter on an empty item ends it) / plain newline |
| `Tab` / `Shift+Tab` | Indent / outdent list item |
| `Ctrl+O` | Open file in a new tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset |

In source view, `(` `[` `{` `"` and backticks auto-close, and typing over a selection with these or `*` `_` `~` `=` wraps the selection. Typing three backticks at the start of a line adds the closing fence.

To uninstall: `mdeer --unregister`, then `rm ~/.local/bin/mdeer` on Linux or `winget uninstall PhilippSchmalen.MDeer` on Windows.

## Build from source

```sh
cargo install --git https://github.com/philippschmalen/mdeer
```

## Development

- Issues are tracked with [beads](https://github.com/gastownhall/beads): `bd ready`
- Git hooks (`.beads/hooks`) run `cargo fmt --check` and clippy on commit, and `cargo test` on push. Commit messages must follow Conventional Commits.
- Perf smoke check (1 MB document): `cargo test --release -- --ignored --nocapture perf`
- Design decisions live in [`docs/adr/`](docs/adr/), terms in [`GLOSSARY.md`](GLOSSARY.md)

## License

[MIT](LICENSE)
