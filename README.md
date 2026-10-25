<p align="center"><img src="assets/logo.svg" width="128" alt="MDeer logo: a deer whose antlers form the letter M"></p>

# MDeer

A lightweight markdown reader and editor, like Notepad but for markdown. Written in Rust, Tokyo Night themed, starts in well under a second.

- Opens files rendered (reading view); `Ctrl+E` switches to a syntax-highlighted source view
- Autosaves as you type; no projects, workspaces or remembered sessions
- One window, files as tabs; opening another file adds a tab to the running window
- Local images, relative links between notes, clickable task checkboxes

## Quickstart (Windows)

In PowerShell:

```powershell
irm https://raw.githubusercontent.com/philippschmalen/mdeer/main/install.ps1 | iex
```

This installs `mdeer.exe` to `%LOCALAPPDATA%\Programs\MDeer`, puts it on your `PATH` and registers it for `.md` files. No admin rights are needed. Then double-click any `.md` file. If Windows already uses another app for `.md`, pick MDeer once under **Open with → Choose another app**.

<details>
<summary>Downloaded <code>mdeer.exe</code> from the releases page and got "Windows protected your PC"?</summary>

MDeer isn't code-signed yet, so SmartScreen blocks browser downloads of it. Click **More info → Run anyway**, or unblock the file first: right-click → **Properties** → tick **Unblock**, or run `Unblock-File .\mdeer.exe`. Then run `mdeer --register` once.

</details>

## Usage

```sh
mdeer notes.md        # open a file
mdeer a.md b.md       # open several as tabs
mdeer                 # open every .md in the current folder as tabs
mdeer --register      # Windows: associate .md files with MDeer
mdeer --unregister    # Windows: remove that association
```

| Shortcut | Action |
|---|---|
| `Ctrl+E` | Toggle reading / source view |
| `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | Bold / italic / link (source view) |
| `Ctrl+O` | Open file in a new tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset |

To uninstall: `mdeer --unregister`, then delete `%LOCALAPPDATA%\Programs\MDeer`.

## Build from source

```sh
cargo install --git https://github.com/philippschmalen/mdeer   # or: cargo build --release
```

Linux and macOS builds work too; `--register` is Windows-only.

## Development

- Issues are tracked with [beads](https://github.com/gastownhall/beads): `bd ready`
- Git hooks (`.beads/hooks`) run `cargo fmt --check` and clippy on commit, and `cargo test` on push. Commit messages must follow Conventional Commits.
- Perf smoke check (1 MB document): `cargo test --release -- --ignored --nocapture perf`
- Design decisions live in [`docs/adr/`](docs/adr/), terms in [`GLOSSARY.md`](GLOSSARY.md)

## License

[MIT](LICENSE)
