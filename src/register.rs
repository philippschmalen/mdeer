//! `mdeer --register` / `--unregister`: per-user (HKCU) `.md` file association, no admin needed.
//!
//! Windows 10+ won't let programs set the default app silently; this makes MDeer the
//! default only if the user never picked one, and always adds it to "Open with".

use std::io;

use winreg::{HKCU, RegKey};

const PROG_ID: &str = "MDeer.md";
const EXTENSIONS: [&str; 2] = [".md", ".markdown"];

pub fn register() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    register_in(&classes()?, &format!("\"{}\" \"%1\"", exe.display()))
}

pub fn unregister() -> io::Result<()> {
    unregister_in(&classes()?)
}

fn classes() -> io::Result<RegKey> {
    Ok(HKCU.create_subkey(r"Software\Classes")?.0)
}

fn register_in(classes: &RegKey, command: &str) -> io::Result<()> {
    let (prog, _) = classes.create_subkey(PROG_ID)?;
    prog.set_value("", &"Markdown document")?;
    prog.create_subkey(r"shell\open\command")?
        .0
        .set_value("", &command)?;
    for ext in EXTENSIONS {
        let (key, _) = classes.create_subkey(ext)?;
        if key
            .get_value::<String, _>("")
            .unwrap_or_default()
            .is_empty()
        {
            key.set_value("", &PROG_ID)?;
        }
        key.create_subkey("OpenWithProgids")?
            .0
            .set_value(PROG_ID, &"")?;
    }
    Ok(())
}

fn unregister_in(classes: &RegKey) -> io::Result<()> {
    for ext in EXTENSIONS {
        let Ok((key, _)) = classes.create_subkey(ext) else {
            continue;
        };
        if key.get_value::<String, _>("").is_ok_and(|v| v == PROG_ID) {
            key.delete_value("")?;
        }
        if let Ok(progids) =
            key.open_subkey_with_flags("OpenWithProgids", winreg::enums::KEY_ALL_ACCESS)
        {
            let _ = progids.delete_value(PROG_ID);
        }
    }
    match classes.delete_subkey_all(PROG_ID) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_roundtrip_in_scratch_key() {
        let root = r"Software\mdeer-test";
        let _ = HKCU.delete_subkey_all(root);
        let (classes, _) = HKCU.create_subkey(root).unwrap();

        register_in(&classes, r#""C:\mdeer.exe" "%1""#).unwrap();
        let cmd: String = classes
            .open_subkey(r"MDeer.md\shell\open\command")
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(cmd, r#""C:\mdeer.exe" "%1""#);
        let default: String = classes.open_subkey(".md").unwrap().get_value("").unwrap();
        assert_eq!(default, PROG_ID);

        unregister_in(&classes).unwrap();
        assert!(classes.open_subkey(PROG_ID).is_err());
        assert!(
            classes
                .open_subkey(".md")
                .unwrap()
                .get_value::<String, _>("")
                .is_err()
        );
        HKCU.delete_subkey_all(root).unwrap();
    }
}
