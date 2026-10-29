//! `mdeer --register` / `--unregister`: per-user desktop integration, no sudo.
//!
//! - Linux: a `.desktop` entry and icon under `~/.local/share`, made the default app
//!   for markdown with `xdg-mime`.
//! - Inside WSL: additionally MDeer under Explorer's "Open with" for `.md`, starting it
//!   through `wsl.exe` under a headless `conhost.exe` (no console window); the user picks "Always" once.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const DESKTOP_FILE: &str = "mdeer.desktop";
const MIME_TYPES: [&str; 2] = ["text/markdown", "text/x-markdown"];
const PROG_ID: &str = "MDeer.md";
const EXTENSIONS: [&str; 2] = [".md", ".markdown"];
/// `wslg.exe` would be the natural launcher, but it exited with -1 in our tests;
/// these two System32 binaries are always present.
const CONHOST: &str = "/mnt/c/Windows/System32/conhost.exe";
const WSL: &str = "/mnt/c/Windows/System32/wsl.exe";

fn data_home() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(dir.into());
    }
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
    Ok(Path::new(&home).join(".local/share"))
}

fn desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=MDeer\nComment=Lightweight markdown reader and editor\n\
         Exec=\"{}\" %F\nIcon=mdeer\nTerminal=false\nMimeType={};\nCategories=Utility;TextEditor;\n\
         StartupWMClass=mdeer\n",
        exe.display(),
        MIME_TYPES.join(";")
    )
}

/// Runs a helper tool; a missing or failing tool becomes a note, not an error.
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    match Command::new(cmd).args(args).output() {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(format!(
            "{cmd} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Some(format!("{cmd} not available ({e})")),
    }
}

/// Returns what was done, one line per step.
pub fn register() -> io::Result<Vec<String>> {
    let exe = std::env::current_exe()?;
    let data = data_home()?;
    let apps = data.join("applications");
    let icons = data.join("icons/hicolor/256x256/apps");
    std::fs::create_dir_all(&apps)?;
    std::fs::create_dir_all(&icons)?;
    std::fs::write(
        icons.join("mdeer.png"),
        include_bytes!("../assets/icon-256.png"),
    )?;
    std::fs::write(apps.join(DESKTOP_FILE), desktop_entry(&exe))?;
    let mut log = vec![format!("Installed {}", apps.join(DESKTOP_FILE).display())];

    let mut args = vec!["default", DESKTOP_FILE];
    args.extend(MIME_TYPES);
    log.push(run("xdg-mime", &args).unwrap_or_else(|| "Set MDeer as default for markdown".into()));
    if let Ok(distro) = std::env::var("WSL_DISTRO_NAME") {
        log.push(windows::register(&exe, &distro)?);
    }
    Ok(log)
}

pub fn unregister() -> io::Result<Vec<String>> {
    let data = data_home()?;
    for path in [
        data.join("applications").join(DESKTOP_FILE),
        data.join("icons/hicolor/256x256/apps/mdeer.png"),
    ] {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let mut log = vec!["Removed the MDeer desktop entry".to_owned()];
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        log.push(windows::unregister()?);
    }
    Ok(log)
}

/// Windows Explorer association, written with `reg.exe` from inside WSL.
mod windows {
    use super::*;

    /// `.reg` string literal escaping.
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }

    /// Imports a `.reg` script. Written as UTF-16 with BOM, which `reg import` expects.
    fn import(script: &str) -> io::Result<()> {
        let path = std::env::temp_dir().join(format!("mdeer-{}.reg", std::process::id()));
        let bytes: Vec<u8> = "\u{feff}Windows Registry Editor Version 5.00\r\n\r\n"
            .encode_utf16()
            .chain(script.replace('\n', "\r\n").encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect();
        std::fs::write(&path, bytes)?;
        let win_path = wslpath_w(&path)?;
        let out = Command::new("reg.exe").args(["import", &win_path]).output();
        let _ = std::fs::remove_file(&path);
        let out = out?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(io::Error::other(format!(
                "reg import failed: {}",
                err.trim()
            )));
        }
        Ok(())
    }

    fn wslpath_w(path: &Path) -> io::Result<String> {
        let out = Command::new("wslpath").arg("-w").arg(path).output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    pub fn register(exe: &Path, distro: &str) -> io::Result<String> {
        if let Some(missing) = [CONHOST, WSL].into_iter().find(|p| !Path::new(p).exists()) {
            return Ok(format!("Skipped Explorer association: {missing} not found"));
        }
        let (conhost, wsl) = (wslpath_w(Path::new(CONHOST))?, wslpath_w(Path::new(WSL))?);
        let command = format!(
            "\"{conhost}\" --headless \"{wsl}\" -d \"{distro}\" -e \"{}\" \"%1\"",
            exe.display()
        );
        let mut script = format!(
            "[HKEY_CURRENT_USER\\Software\\Classes\\{PROG_ID}]\n@=\"Markdown document\"\n\n\
             [HKEY_CURRENT_USER\\Software\\Classes\\{PROG_ID}\\shell\\open\\command]\n@={}\n\n",
            quote(&command)
        );
        for ext in EXTENSIONS {
            script += &format!(
                "[HKEY_CURRENT_USER\\Software\\Classes\\{ext}\\OpenWithProgids]\n\"{PROG_ID}\"=\"\"\n\n"
            );
        }
        import(&script)?;
        Ok("Added MDeer to Explorer's \"Open with\" for .md files (runs via WSL)".into())
    }

    pub fn unregister() -> io::Result<String> {
        let mut script = format!("[-HKEY_CURRENT_USER\\Software\\Classes\\{PROG_ID}]\n\n");
        for ext in EXTENSIONS {
            script += &format!(
                "[HKEY_CURRENT_USER\\Software\\Classes\\{ext}\\OpenWithProgids]\n\"{PROG_ID}\"=-\n\n"
            );
        }
        import(&script)?;
        Ok("Removed the Windows Explorer association".into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reg_strings_escape_quotes_and_backslashes() {
            assert_eq!(quote(r#""C:\a b" "%1""#), r#""\"C:\\a b\" \"%1\"""#);
        }
    }
}

/// Explorer hands WSL apps Windows paths; map them into the Linux filesystem.
// ponytail: assumes WSL's default `/mnt` automount root; use `wslpath -u` if someone changes it.
pub fn from_windows_path(arg: &str) -> Option<PathBuf> {
    let unix = |rest: &str| rest.replace('\\', "/");
    for prefix in [r"\\wsl.localhost\", r"\\wsl$\"] {
        if let Some(rest) = arg.strip_prefix(prefix) {
            let (_distro, path) = rest.split_once('\\')?;
            return Some(PathBuf::from(format!("/{}", unix(path))));
        }
    }
    let b = arg.as_bytes();
    let drive_path =
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/');
    drive_path.then(|| {
        PathBuf::from(format!(
            "/mnt/{}/{}",
            (b[0] as char).to_ascii_lowercase(),
            unix(&arg[3..])
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_map_into_wsl() {
        assert_eq!(
            from_windows_path(r"C:\Users\me\My Notes\a.md"),
            Some(PathBuf::from("/mnt/c/Users/me/My Notes/a.md"))
        );
        assert_eq!(
            from_windows_path(r"\\wsl.localhost\Ubuntu-24.04\home\me\a.md"),
            Some(PathBuf::from("/home/me/a.md"))
        );
        assert_eq!(
            from_windows_path(r"\\wsl$\Ubuntu\tmp\a.md"),
            Some(PathBuf::from("/tmp/a.md"))
        );
        assert_eq!(from_windows_path("notes/a.md"), None);
        assert_eq!(from_windows_path("/home/me/a.md"), None);
    }

    #[test]
    fn desktop_entry_quotes_exec() {
        let entry = desktop_entry(Path::new("/home/me/.local/bin/mdeer"));
        assert!(entry.contains("Exec=\"/home/me/.local/bin/mdeer\" %F\n"));
        assert!(entry.contains("MimeType=text/markdown;text/x-markdown;\n"));
    }
}
