#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod source;
mod theme;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};

struct Doc {
    path: PathBuf,
    text: String,
    cache: CommonMarkCache,
    /// `file://<dir>/` so relative image paths resolve next to the file.
    image_base: String,
    editing: bool,
    /// Source-view highlighting, keyed by a hash of text and font.
    highlight: (u64, LayoutJob),
}

impl Doc {
    fn open(path: &Path) -> std::io::Result<Self> {
        // `absolute`, not `canonicalize`: Windows verbatim `\\?\` paths break `file://` image URIs.
        let path = std::path::absolute(path)?;
        let text = std::fs::read_to_string(&path)?;
        let dir = path.parent().unwrap_or(Path::new("."));
        // egui's file loader wants `file:///C:/dir` on Windows and `file:///dir` elsewhere.
        let dir = dir.to_string_lossy().replace('\\', "/");
        let slash = if dir.starts_with('/') { "" } else { "/" };
        Ok(Self {
            image_base: format!("file://{slash}{dir}/"),
            path,
            text,
            cache: CommonMarkCache::default(),
            editing: false,
            highlight: Default::default(),
        })
    }

    fn title(&self) -> String {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        format!("{name} — MDeer")
    }
}

struct App {
    doc: Doc,
}

const CTRL: Modifiers = Modifiers::COMMAND;

fn pressed(ui: &egui::Ui, key: Key) -> bool {
    ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(CTRL, key)))
}

impl Doc {
    fn id(&self) -> egui::Id {
        egui::Id::unique(&self.path)
    }

    fn reading_view(&mut self, ui: &mut egui::Ui) {
        CommonMarkViewer::new()
            .default_implicit_uri_scheme(&self.image_base)
            .enable_scroll_to_heading(true)
            .show_scrollable(
                self.id(),
                ui,
                &mut self.cache,
                &CommonMarkScrollOptions::default().viewport_cache(true),
                &self.text,
            );
    }

    fn source_view(&mut self, ui: &mut egui::Ui, focus: bool) {
        let id = self.id().with("source");
        for (key, before, after) in [
            (Key::B, "**", "**"),
            (Key::I, "*", "*"),
            (Key::K, "[", "]()"),
        ] {
            if pressed(ui, key) {
                self.format_selection(ui.ctx(), id, before, after);
            }
        }

        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let cache = &mut self.highlight;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let key = egui::util::hash((buf.as_str(), &font));
            if cache.0 != key {
                *cache = (key, source::highlight(buf.as_str(), &font));
            }
            let mut job = cache.1.clone();
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            let response = egui::TextEdit::multiline(&mut self.text)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui)
                .response;
            if focus {
                response.request_focus();
            }
        });
    }

    /// Obsidian-style Ctrl+B/I/K: wrap the selection (or insert markers at the cursor).
    fn format_selection(&mut self, ctx: &egui::Context, id: egui::Id, before: &str, after: &str) {
        let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        let sel = range.as_sorted_char_range();
        let new = source::wrap(&mut self.text, sel.start.0..sel.end.0, before, after);
        let new = if after == "]()" {
            // Link: put the cursor inside the empty `()`.
            CCursorRange::one(CCursor::new(new.end + 2))
        } else {
            CCursorRange::two(CCursor::new(new.start), CCursor::new(new.end))
        };
        state.cursor.set_char_range(Some(new));
        state.store(ctx, id);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let doc = &mut self.doc;
        let toggled = pressed(ui, Key::E);
        if toggled {
            doc.editing = !doc.editing;
            if !doc.editing {
                // The viewport cache doesn't notice text changes on its own.
                doc.cache.clear_viewer(&doc.id());
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if doc.editing {
                doc.source_view(ui, toggled);
            } else {
                doc.reading_view(ui);
            }
        });
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file] = args.as_slice() else {
        eprintln!("usage: mdeer <file>");
        return ExitCode::FAILURE;
    };
    let doc = match Doc::open(Path::new(file)) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("mdeer: {file}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(doc.title())
            .with_app_id("mdeer")
            .with_inner_size([900.0, 1000.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "mdeer",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(App { doc }))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mdeer: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_image_loads_next_to_file() {
        let dir = std::env::temp_dir().join("mdeer-test-image");
        std::fs::create_dir_all(dir.join("img")).unwrap();
        let mut png = Vec::new();
        image::RgbImage::new(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.join("img/a.png"), png).unwrap();
        std::fs::write(dir.join("doc.md"), "![a](img/a.png)").unwrap();

        let doc = Doc::open(&dir.join("doc.md")).unwrap();
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        // Same concatenation egui_commonmark does for scheme-less image URIs.
        let uri = format!("{}img/a.png", doc.image_base);
        for _ in 0..200 {
            match ctx.try_load_image(&uri, egui::SizeHint::default()) {
                Ok(egui::load::ImagePoll::Ready { .. }) => return,
                Ok(egui::load::ImagePoll::Pending { .. }) => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(e) => panic!("{uri}: {e}"),
            }
        }
        panic!("{uri}: still pending");
    }
}
