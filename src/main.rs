#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(unix)]
mod desktop;
mod doc;
mod instance;
#[cfg(windows)]
mod register;
mod source;
mod theme;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::Receiver;

use doc::Doc;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, OutputCommand, ViewportCommand};

struct App {
    docs: Vec<Doc>,
    active: usize,
    title: String,
    /// Files forwarded by later `mdeer` invocations.
    inbox: Option<Receiver<PathBuf>>,
}

const CTRL: Modifiers = Modifiers::COMMAND;

fn pressed(ui: &egui::Ui, key: Key) -> bool {
    pressed_with(ui, CTRL, key)
}

fn pressed_with(ui: &egui::Ui, modifiers: Modifiers, key: Key) -> bool {
    ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Decodes `%XX` escapes (e.g. `My%20Note.md`); invalid escapes stay as-is.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) if b[i] == b'%' => {
                out.push(byte);
                i += 3;
            }
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A link without a URI scheme, resolved against `dir`: `(path, #anchor)`.
fn local_link(dir: &Path, url: &str) -> Option<(PathBuf, Option<String>)> {
    let scheme = url
        .find(':')
        .is_some_and(|c| !url[..c].contains(['/', '#']));
    if scheme || url.starts_with('#') {
        return None;
    }
    let (path, anchor) = match url.split_once('#') {
        Some((p, a)) => (p, Some(a.to_owned())),
        None => (url, None),
    };
    Some((dir.join(percent_decode(path)), anchor))
}

impl App {
    /// Focus `path` if it is already open, else open it in a new tab.
    fn open(&mut self, path: &Path) -> std::io::Result<()> {
        let canonical = std::fs::canonicalize(path)?;
        let existing = self
            .docs
            .iter()
            .position(|d| std::fs::canonicalize(&d.path).is_ok_and(|p| p == canonical));
        self.active = match existing {
            Some(i) => i,
            None => {
                self.docs.push(Doc::open(path)?);
                self.docs.len() - 1
            }
        };
        Ok(())
    }

    fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"]);
        if let Some(dir) = self.docs[self.active].path.parent() {
            dialog = dialog.set_directory(dir);
        }
        for path in dialog.pick_files().unwrap_or_default() {
            if let Err(e) = self.open(&path) {
                self.docs[self.active].error = Some(format!("{}: {e}", path.display()));
            }
        }
    }

    /// Returns false once the last tab is closed.
    fn close_active(&mut self) -> bool {
        let mut doc = self.docs.remove(self.active);
        doc.autosave(true);
        self.active = self.active.min(self.docs.len().saturating_sub(1));
        !self.docs.is_empty()
    }

    fn shortcuts(&mut self, ui: &egui::Ui) {
        let n = self.docs.len();
        if pressed(ui, Key::O) {
            self.open_dialog();
        }
        if pressed(ui, Key::W) && !self.close_active() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        // Shift first: consume_shortcut ignores extra Shift when matching Ctrl+Tab.
        if pressed_with(ui, CTRL | Modifiers::SHIFT, Key::Tab) {
            self.active = (self.active + n - 1) % n;
        } else if pressed(ui, Key::Tab) {
            self.active = (self.active + 1) % n;
        }
    }

    /// Only shown with more than one file open.
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        if self.docs.len() < 2 {
            return;
        }
        let mut close = None;
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, doc) in self.docs.iter().enumerate() {
                    let tab = ui.selectable_label(i == self.active, doc.name());
                    if tab.clicked() {
                        self.active = i;
                    }
                    if tab.middle_clicked() || ui.small_button("×").clicked() {
                        close = Some(i);
                    }
                }
            });
        });
        if let Some(i) = close {
            self.active = i;
            self.close_active();
        }
    }

    fn receive_forwarded(&mut self, ctx: &egui::Context) {
        let Some(inbox) = &self.inbox else { return };
        let paths: Vec<PathBuf> = inbox.try_iter().collect();
        if paths.is_empty() {
            return;
        }
        for path in paths {
            if let Err(e) = self.open(&path) {
                self.docs[self.active].error = Some(format!("{}: {e}", path.display()));
            }
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Conflict or save error, with the actions that resolve it.
    fn banner(&mut self, ui: &mut egui::Ui) {
        let doc = &mut self.docs[self.active];
        if !doc.conflict && doc.error.is_none() {
            return;
        }
        egui::Panel::top("banner").show(ui, |ui| {
            ui.horizontal(|ui| {
                if doc.conflict {
                    ui.colored_label(
                        theme::YELLOW,
                        "File changed on disk while you were editing.",
                    );
                    if ui.button("Reload from disk").clicked() {
                        doc.reload();
                    }
                    if ui.button("Keep mine").clicked() {
                        doc.save();
                    }
                } else if let Some(e) = &doc.error {
                    ui.colored_label(theme::RED, e);
                }
            });
        });
    }

    /// Relative links open as tabs (markdown) or with the system default app
    /// (anything else); `scheme://` links are left for egui to open in the browser.
    fn follow_local_links(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.docs[self.active].path.parent().map(Path::to_path_buf) else {
            return;
        };
        let mut targets = Vec::new();
        ctx.output_mut(|o| {
            for cmd in &mut o.commands {
                if let OutputCommand::OpenUrl(open) = cmd
                    && let Some((path, anchor)) = local_link(&dir, &open.url)
                {
                    if is_markdown(&path) {
                        targets.push((path, anchor));
                        open.url.clear();
                    } else {
                        open.url = doc::file_url(&path);
                    }
                }
            }
            o.commands
                .retain(|c| !matches!(c, OutputCommand::OpenUrl(open) if open.url.is_empty()));
        });
        for (path, anchor) in targets {
            match self.open(&path) {
                Ok(()) => {
                    if let Some(anchor) = anchor {
                        self.docs[self.active].scroll_to(anchor);
                    }
                }
                Err(e) => self.docs[self.active].error = Some(format!("{}: {e}", path.display())),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive_forwarded(ui.ctx());
        let focus_change = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::WindowFocused(f) => Some(*f),
                _ => None,
            })
        });
        for doc in &mut self.docs {
            match focus_change {
                Some(true) => doc.sync_from_disk(),
                Some(false) => doc.autosave(true),
                None => doc.autosave(false),
            }
            if doc.edited_at.is_some() {
                ui.ctx().request_repaint_after(doc::AUTOSAVE_DELAY);
            }
        }

        self.shortcuts(ui);
        if self.docs.is_empty() {
            return;
        }
        let toggled = pressed(ui, Key::E);
        if toggled {
            self.docs[self.active].toggle_editing();
        }
        self.tab_bar(ui);
        self.banner(ui);
        let doc = &mut self.docs[self.active];
        egui::CentralPanel::default().show(ui, |ui| {
            if doc.editing {
                doc.source_view(ui, toggled);
            } else {
                doc.reading_view(ui);
            }
        });
        self.follow_local_links(ui.ctx());

        let title = self.docs[self.active].title();
        if title != self.title {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for doc in &mut self.docs {
            doc.autosave(true);
        }
    }
}

/// `mdeer <file>...` opens those files; bare `mdeer` opens every `.md` in the current directory.
fn files_from_args(args: &[String]) -> Result<Vec<PathBuf>, String> {
    if !args.is_empty() {
        #[cfg(unix)]
        let path = |a: &String| desktop::from_windows_path(a).unwrap_or_else(|| PathBuf::from(a));
        #[cfg(windows)]
        let path = PathBuf::from;
        return Ok(args.iter().map(path).collect());
    }
    let dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_file() && is_markdown(p))
        .collect();
    files.sort_by_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    if files.is_empty() {
        return Err(format!("no .md files in {}", dir.display()));
    }
    Ok(files)
}

fn window_icon() -> egui::IconData {
    let icon = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
        .expect("bundled icon decodes")
        .into_rgba8();
    egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    }
}

/// Report to stderr, and in a dialog on Windows where release builds have no console.
fn fail(msg: &str) -> ExitCode {
    eprintln!("mdeer: {msg}");
    #[cfg(windows)]
    rfd::MessageDialog::new()
        .set_title("MDeer")
        .set_description(msg)
        .set_level(rfd::MessageLevel::Error)
        .show();
    ExitCode::FAILURE
}

/// `--register` / `--unregister`; `None` if `flag` is not one of them.
fn run_flag(flag: &str) -> Option<ExitCode> {
    #[cfg(unix)]
    use desktop as platform;
    #[cfg(windows)]
    use register as platform;
    let result = match flag {
        "--register" => platform::register(),
        "--unregister" => platform::unregister(),
        _ => return None,
    };
    Some(match result {
        Ok(log) => {
            log.iter().for_each(|line| println!("{line}"));
            #[cfg(windows)]
            rfd::MessageDialog::new()
                .set_title("MDeer")
                .set_description(log.join("\n"))
                .show();
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    })
}

/// WSLg has no DRM render node: Mesa probes for one, tries Zink, and only then
/// falls back, printing warnings at each step. Pin its native D3D12 driver and
/// mute EGL's probe noise. Re-exec because `set_var` is unsafe.
#[cfg(unix)]
fn wslg_gl() {
    use std::os::unix::process::CommandExt;
    if std::env::var_os("GALLIUM_DRIVER").is_some()
        || !Path::new("/usr/lib/wsl/lib/libd3d12.so").exists()
    {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // exec only returns on failure; then carry on with Mesa's defaults.
    let _ = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("GALLIUM_DRIVER", "d3d12")
        .env("EGL_LOG_LEVEL", "fatal")
        .exec();
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag] = args.as_slice()
        && let Some(code) = run_flag(flag)
    {
        return code;
    }
    let files = match files_from_args(&args) {
        Ok(files) => files,
        Err(e) => {
            return fail(&e.to_string());
        }
    };
    // The running instance has another working directory.
    let files: Vec<PathBuf> = files
        .iter()
        .map(|f| std::path::absolute(f).unwrap_or_else(|_| f.clone()))
        .collect();
    let socket = instance::socket_name();
    if instance::forward(&socket, &files) {
        return ExitCode::SUCCESS;
    }
    #[cfg(unix)]
    wslg_gl();
    let listener = instance::bind(&socket);

    let mut app = App {
        docs: Vec::new(),
        active: 0,
        title: String::new(),
        inbox: None,
    };
    for file in &files {
        if let Err(e) = app.open(file) {
            return fail(&format!("{}: {e}", file.display()));
        }
    }
    app.active = 0;
    app.title = app.docs[0].title();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&app.title)
            .with_app_id("mdeer")
            .with_inner_size([900.0, 1000.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "mdeer",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            // egui's 12.5pt body text is tiny at 1x; Ctrl +/- still adjusts from here.
            cc.egui_ctx.set_zoom_factor(1.8);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            app.inbox = listener.map(|l| instance::serve(l, move || ctx.request_repaint()));
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_links() {
        let dir = Path::new("/notes");
        assert_eq!(local_link(dir, "https://x.org/a.md"), None);
        assert_eq!(local_link(dir, "mailto:a@b.c"), None);
        assert_eq!(local_link(dir, "#top"), None);
        assert_eq!(
            local_link(dir, "sub/My%20Note.md#intro"),
            Some((dir.join("sub/My Note.md"), Some("intro".into())))
        );
        assert_eq!(
            local_link(dir, "../a.md"),
            Some((dir.join("../a.md"), None))
        );
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b%zz%"), "a b%zz%");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }
}
