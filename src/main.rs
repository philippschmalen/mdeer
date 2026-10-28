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
    pending_close: Option<PendingClose>,
}

enum CloseTarget {
    Tab(PathBuf),
    Window,
}

struct PendingClose {
    target: CloseTarget,
    failed: Vec<PathBuf>,
    confirming: Option<PathBuf>,
    /// Approval applies only to this document's exact pending text.
    discarded: Vec<(PathBuf, String)>,
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
        if let Some(dir) = self.docs.get(self.active).and_then(|d| d.path.parent()) {
            dialog = dialog.set_directory(dir);
        }
        for path in dialog.pick_files().unwrap_or_default() {
            if let Err(e) = self.open(&path) {
                if let Some(doc) = self.docs.get_mut(self.active) {
                    doc.error = Some(format!("{}: {e}", path.display()));
                } else {
                    eprintln!("mdeer: {}: {e}", path.display());
                }
            }
        }
    }

    fn request_close(&mut self, target: CloseTarget, ctx: &egui::Context) {
        self.finish_close(
            PendingClose {
                target,
                failed: Vec::new(),
                confirming: None,
                discarded: Vec::new(),
            },
            ctx,
        );
    }

    fn finish_close(&mut self, mut pending: PendingClose, ctx: &egui::Context) {
        pending.failed.clear();
        pending.confirming = None;
        for doc in &mut self.docs {
            if matches!(&pending.target, CloseTarget::Tab(path) if path != &doc.path) {
                continue;
            }
            let discarded = pending
                .discarded
                .iter()
                .any(|(path, text)| path == &doc.path && text == &doc.text);
            if !discarded && !doc.autosave(true) {
                pending.failed.push(doc.path.clone());
            }
        }
        if let Some(first) = pending.failed.first() {
            self.active = self
                .docs
                .iter()
                .position(|doc| &doc.path == first)
                .unwrap_or(0);
            self.docs[self.active].editing = true;
            self.pending_close = Some(pending);
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            return;
        }
        match pending.target {
            CloseTarget::Tab(path) => self.docs.retain(|doc| doc.path != path),
            CloseTarget::Window => self.docs.clear(),
        }
        self.pending_close = None;
        self.active = self.active.min(self.docs.len().saturating_sub(1));
        if self.docs.is_empty() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn close_active(&mut self, ctx: &egui::Context) {
        if let Some(doc) = self.docs.get(self.active) {
            self.request_close(CloseTarget::Tab(doc.path.clone()), ctx);
        }
    }

    fn viewport_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.request_close(CloseTarget::Window, ctx);
        }
    }

    fn shortcuts(&mut self, ui: &egui::Ui) {
        if pressed(ui, Key::O) {
            self.open_dialog();
        }
        if pressed(ui, Key::W) {
            self.close_active(ui.ctx());
        }
        let n = self.docs.len();
        if n == 0 {
            return;
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
            self.close_active(ui.ctx());
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
                if let Some(doc) = self.docs.get_mut(self.active) {
                    doc.error = Some(format!("{}: {e}", path.display()));
                } else {
                    eprintln!("mdeer: {}: {e}", path.display());
                }
            }
        }
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn close_banner(&mut self, ui: &mut egui::Ui) {
        let Some(pending) = &mut self.pending_close else {
            return;
        };
        let mut retry = false;
        let mut cancel = false;
        let mut confirmed = false;
        egui::Frame::side_top_panel(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.colored_label(
                theme::RED,
                "Close stopped. All pending edits are still open.",
            );
            for path in &pending.failed {
                ui.label(path.display().to_string());
            }
            ui.horizontal_wrapped(|ui| {
                retry = ui.button("Retry close").clicked();
                cancel = ui.button("Cancel close").clicked();
                if let Some(path) = pending.confirming.clone() {
                    if ui
                        .button(format!("Confirm discard unsaved edits: {}", path.display()))
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("Keep edits").clicked() {
                        pending.confirming = None;
                    }
                } else if let Some(path) = pending.failed.first()
                    && ui
                        .button(format!("Discard unsaved edits for {}…", path.display()))
                        .clicked()
                {
                    pending.confirming = Some(path.clone());
                }
            });
        });
        if cancel {
            self.pending_close = None;
        } else if confirmed {
            self.confirm_discard(ui.ctx());
        } else if retry {
            let pending = self.pending_close.take().expect("pending close");
            self.finish_close(pending, ui.ctx());
        }
    }

    fn confirm_discard(&mut self, ctx: &egui::Context) {
        let Some(pending) = &mut self.pending_close else {
            return;
        };
        let Some(path) = pending.confirming.take() else {
            return;
        };
        if !pending.failed.contains(&path) {
            return;
        }
        let Some(doc) = self.docs.iter().find(|doc| doc.path == path) else {
            return;
        };
        pending.discarded.push((path, doc.text.clone()));
        let pending = self.pending_close.take().expect("pending close");
        self.finish_close(pending, ctx);
    }

    /// Region controls stay above the existing Source editor.
    fn banner(&mut self, ui: &mut egui::Ui) {
        let doc = &mut self.docs[self.active];
        if !doc.has_conflicts() && doc.error.is_none() {
            return;
        }
        egui::Frame::side_top_panel(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if doc.has_conflicts() {
                    let count = doc.conflict_count();
                    ui.colored_label(
                        theme::YELLOW,
                        if count == 0 {
                            "Incomplete MDeer markers: finish editing in Source.".to_owned()
                        } else {
                            format!("MDeer conflicts: region 1 of {count}. Edit Source or choose:")
                        },
                    );
                    if count > 0 {
                        for (label, resolution) in [
                            ("Keep mine", doc::Resolution::Mine),
                            ("Use disk", doc::Resolution::Disk),
                            ("Keep both", doc::Resolution::Both),
                        ] {
                            if ui.button(label).clicked() {
                                doc.resolve_first(resolution);
                            }
                        }
                    }
                }
            });
            if let Some(e) = doc.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        if doc.merge_paused {
                            theme::YELLOW
                        } else {
                            theme::RED
                        },
                        e,
                    );
                    if ui.button("Retry save").clicked() {
                        doc.autosave(true);
                    }
                });
            }
            if doc.has_conflicts() && !doc.editing && ui.button("Show Source").clicked() {
                doc.editing = true;
            }
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
        self.viewport_close(ui.ctx());
        if self.docs.is_empty() {
            return;
        }
        let focus_change = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::WindowFocused(f) => Some(*f),
                _ => None,
            })
        });
        for doc in &mut self.docs {
            match focus_change {
                Some(true) => doc.sync_from_disk(),
                Some(false) => {
                    doc.autosave(true);
                }
                None => {
                    doc.autosave(false);
                }
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
        self.close_banner(ui);
        if self.docs.is_empty() {
            return;
        }
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

/// `--version` / `--register` / `--unregister`; `None` if `flag` is not one of them.
fn run_flag(flag: &str) -> Option<ExitCode> {
    // No window or dialog: winget's validation runs this to check the exe starts.
    if flag == "--version" {
        println!("mdeer {}", env!("CARGO_PKG_VERSION"));
        return Some(ExitCode::SUCCESS);
    }
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
        pending_close: None,
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

    struct TestFiles(PathBuf);

    impl Drop for TestFiles {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn app_fixture(name: &str, texts: &[&str]) -> (App, TestFiles) {
        let dir = std::env::current_dir()
            .unwrap()
            .join(".test-artifacts")
            .join(format!("app-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let docs = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let path = dir.join(format!("{i}.md"));
                std::fs::write(&path, text).unwrap();
                Doc::open(&path).unwrap()
            })
            .collect();
        (
            App {
                docs,
                active: 0,
                title: String::new(),
                inbox: None,
                pending_close: None,
            },
            TestFiles(dir),
        )
    }

    fn edit(doc: &mut Doc, text: &str) {
        doc.text = text.to_owned();
        doc.edited_at = Some(std::time::Instant::now());
    }

    fn fail_reads(doc: &Doc) {
        std::fs::remove_file(&doc.path).unwrap();
        std::fs::create_dir(&doc.path).unwrap();
    }

    fn viewport_command(output: &egui::FullOutput, command: ViewportCommand) -> bool {
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&command)
    }

    fn click_label(
        ctx: &egui::Context,
        label: &str,
        button: egui::PointerButton,
        mut show: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        let screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 1000.0),
        ));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect,
                ..Default::default()
            },
            &mut show,
        );
        output.textures_delta.clear();
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                panic!("missing visible label: {label}; found {labels:?}");
            });
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect,
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            show,
        );
        output.textures_delta.clear();
        output
    }

    #[test]
    fn tab_x_and_middle_click_both_retain_a_tab_with_failed_edits() {
        for (name, button, label) in [
            ("tab-x", egui::PointerButton::Primary, "×"),
            ("tab-middle", egui::PointerButton::Middle, "0.md"),
        ] {
            let (mut app, _files) = app_fixture(name, &["a", "b"]);
            edit(&mut app.docs[0], "pending");
            fail_reads(&app.docs[0]);
            let ctx = egui::Context::default();
            let output = click_label(&ctx, label, button, |ui| app.tab_bar(ui));
            assert!(viewport_command(&output, ViewportCommand::CancelClose));
            assert_eq!(app.docs.len(), 2);
            assert_eq!(app.docs[0].text, "pending");
            assert_eq!(app.docs[1].text, "b");
            assert!(app.pending_close.is_some());
        }
    }

    #[test]
    fn ctrl_w_retains_failed_last_tab_until_specific_discard_is_confirmed() {
        let (mut app, _files) = app_fixture("ctrl-w", &["base"]);
        edit(&mut app.docs[0], "pending");
        fail_reads(&app.docs[0]);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::W,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: CTRL,
                }],
                ..Default::default()
            },
            |ui| app.shortcuts(ui),
        );
        output.textures_delta.clear();
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.docs[0].text, "pending");
        assert!(viewport_command(&output, ViewportCommand::CancelClose));
        assert!(!viewport_command(&output, ViewportCommand::Close));
        app.confirm_discard(&ctx);
        assert_eq!(app.docs.len(), 1, "discard needs an explicit confirmation");
        let path = app.docs[0].path.clone();
        click_label(
            &ctx,
            &format!("Discard unsaved edits for {}…", path.display()),
            egui::PointerButton::Primary,
            |ui| app.close_banner(ui),
        );
        assert_eq!(app.docs.len(), 1);
        let output = click_label(
            &ctx,
            &format!("Confirm discard unsaved edits: {}", path.display()),
            egui::PointerButton::Primary,
            |ui| app.close_banner(ui),
        );
        assert!(app.docs.is_empty());
        assert!(viewport_command(&output, ViewportCommand::Close));
        ctx.run_ui(Default::default(), |ui| app.shortcuts(ui))
            .textures_delta
            .clear();
    }

    #[test]
    fn late_failed_open_after_last_tab_closes_never_indexes_an_empty_list() {
        let (mut app, files) = app_fixture("late-open", &["base"]);
        let ctx = egui::Context::default();
        app.close_active(&ctx);
        assert!(app.docs.is_empty());
        let (sender, inbox) = std::sync::mpsc::channel();
        app.inbox = Some(inbox);
        sender.send(files.0.join("missing.md")).unwrap();
        ctx.run_ui(Default::default(), |ui| {
            app.receive_forwarded(ui.ctx());
            app.shortcuts(ui);
        })
        .textures_delta
        .clear();
        assert!(app.docs.is_empty());
    }

    #[test]
    fn native_window_close_keeps_all_tabs_after_partial_save_and_scopes_discard() {
        let (mut app, _files) = app_fixture("window", &["a", "b", "c"]);
        for (i, doc) in app.docs.iter_mut().enumerate() {
            edit(doc, &format!("pending {i}"));
        }
        fail_reads(&app.docs[1]);
        fail_reads(&app.docs[2]);
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| app.viewport_close(ui.ctx()));
        output.textures_delta.clear();
        assert!(viewport_command(&output, ViewportCommand::CancelClose));
        assert_eq!(app.docs.len(), 3);
        assert_eq!(app.active, 1);
        assert!(app.docs[0].edited_at.is_none());
        assert_eq!(
            std::fs::read_to_string(&app.docs[0].path).unwrap(),
            "pending 0",
            "earlier successful saves remain; close is not a transaction"
        );
        assert_eq!(app.pending_close.as_ref().unwrap().failed.len(), 2);
        app.pending_close.as_mut().unwrap().confirming = Some(app.docs[1].path.clone());
        app.confirm_discard(&ctx);
        assert_eq!(app.docs.len(), 3);
        assert_eq!(
            app.pending_close.as_ref().unwrap().failed,
            vec![app.docs[2].path.clone()],
            "confirming one file cannot discard another"
        );
        assert_eq!(app.docs[2].text, "pending 2");
        // A new edit invalidates an earlier document-specific discard approval.
        edit(&mut app.docs[1], "new edits after approval");
        app.pending_close.as_mut().unwrap().confirming = Some(app.docs[2].path.clone());
        app.confirm_discard(&ctx);
        assert_eq!(app.docs.len(), 3);
        assert_eq!(
            app.pending_close.as_ref().unwrap().failed,
            vec![app.docs[1].path.clone()]
        );
        app.pending_close.as_mut().unwrap().confirming = Some(app.docs[1].path.clone());
        let mut output = ctx.run_ui(Default::default(), |ui| app.confirm_discard(ui.ctx()));
        output.textures_delta.clear();
        assert!(app.docs.is_empty());
        assert!(viewport_command(&output, ViewportCommand::Close));
    }

    #[test]
    fn failed_tab_close_retains_neighbors_and_retry_saves_before_removal() {
        let (mut app, _files) = app_fixture("tab-retry", &["a", "b"]);
        app.active = 1;
        edit(&mut app.docs[1], "pending");
        let path = app.docs[1].path.clone();
        fail_reads(&app.docs[1]);
        let ctx = egui::Context::default();
        app.close_active(&ctx);
        assert_eq!(app.docs.len(), 2);
        assert_eq!(app.active, 1);
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "b").unwrap();
        click_label(&ctx, "Retry close", egui::PointerButton::Primary, |ui| {
            app.close_banner(ui)
        });
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.docs[0].text, "a");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "pending");
        assert!(app.pending_close.is_none());
        assert_eq!(app.active, 0);
    }

    #[test]
    fn region_buttons_resolve_only_one_region_above_the_existing_source_editor() {
        let first = "<<<<<<< MDeer: yours\nmine α\n=======\ndisk β\n>>>>>>> MDeer: external\n";
        let second =
            "<<<<<<< MDeer: yours\nsecond mine\n=======\nsecond disk\n>>>>>>> MDeer: external\n";
        for (name, label, expected) in [
            ("mine", "Keep mine", "mine α\n"),
            ("disk", "Use disk", "disk β\n"),
            ("both", "Keep both", "mine α\ndisk β\n"),
        ] {
            let (mut app, _files) = app_fixture(name, &[&format!("{first}middle\n{second}")]);
            let ctx = egui::Context::default();
            click_label(&ctx, label, egui::PointerButton::Primary, |ui| {
                app.banner(ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    app.docs[0].source_view(ui, false);
                });
            });
            assert_eq!(app.docs[0].text, format!("{expected}middle\n{second}"));
            assert_eq!(app.docs[0].conflict_count(), 1);
            assert!(app.docs[0].autosave(true));
            assert_eq!(
                std::fs::read_to_string(&app.docs[0].path).unwrap(),
                app.docs[0].text
            );
            assert!(app.docs[0].has_conflicts());
        }
    }

    #[test]
    fn saved_conflicts_allow_normal_window_close_and_cancel_preserves_dirty_text() {
        let marked = "<<<<<<< MDeer: yours\nmine\n=======\ndisk\n>>>>>>> MDeer: external\n";
        let (mut app, _files) = app_fixture("markers-close", &[marked]);
        assert!(app.docs[0].editing);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            app.request_close(CloseTarget::Window, ui.ctx());
        });
        output.textures_delta.clear();
        assert!(app.docs.is_empty());
        assert!(viewport_command(&output, ViewportCommand::Close));
        assert!(!viewport_command(&output, ViewportCommand::CancelClose));

        let (mut app, _files) = app_fixture("cancel-close", &["base"]);
        let ctx = egui::Context::default();
        edit(&mut app.docs[0], "keep editing");
        fail_reads(&app.docs[0]);
        app.request_close(CloseTarget::Window, &ctx);
        click_label(&ctx, "Cancel close", egui::PointerButton::Primary, |ui| {
            app.close_banner(ui)
        });
        assert!(app.pending_close.is_none());
        assert_eq!(app.docs[0].text, "keep editing");
        assert!(app.docs[0].edited_at.is_some());
        assert_eq!(app.docs.len(), 1);
    }

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
