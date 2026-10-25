//! One open markdown file: its text, views and autosave state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Key};
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};

use crate::{pressed, source};

pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);
/// Above this, the reading view is virtualized and task checkboxes are read-only.
const CHECKBOX_MAX_BYTES: usize = 200_000;

/// What we last saw on disk, to notice writes by other programs.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// `file:///C:/dir` on Windows, `file:///dir` elsewhere (the form egui's file loader expects).
pub fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let slash = if path.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{path}")
}

pub struct Doc {
    pub path: PathBuf,
    pub text: String,
    cache: CommonMarkCache,
    /// `file://<dir>/` so relative image paths resolve next to the file.
    image_base: String,
    pub editing: bool,
    /// Source-view highlighting, keyed by a hash of text and font.
    highlight: (u64, LayoutJob),
    /// Time of the latest edit not yet on disk (autosave debounces on it).
    pub edited_at: Option<Instant>,
    disk: Stamp,
    /// The file changed on disk while we had unsaved edits; autosave pauses.
    pub conflict: bool,
    pub error: Option<String>,
}

impl Doc {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        // `absolute`, not `canonicalize`: Windows verbatim `\\?\` paths break `file://` image URIs.
        let path = std::path::absolute(path)?;
        let text = std::fs::read_to_string(&path)?;
        Ok(Self {
            image_base: file_url(path.parent().unwrap_or(Path::new("."))) + "/",
            disk: stamp(&path),
            path,
            text,
            cache: CommonMarkCache::default(),
            editing: false,
            highlight: Default::default(),
            edited_at: None,
            conflict: false,
            error: None,
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn title(&self) -> String {
        format!("{} — MDeer", self.name())
    }

    /// Scroll the reading view to the heading with this `{#anchor}`.
    pub fn scroll_to(&mut self, anchor: String) {
        // `show_mut` has no viewer id, so it looks under `Id::NULL`.
        self.cache
            .scroll_to_heading(&egui::Id::NULL, Some(anchor.clone()));
        self.cache.scroll_to_heading(&self.id(), Some(anchor));
    }

    fn id(&self) -> egui::Id {
        egui::Id::unique(&self.path)
    }

    fn mark_edited(&mut self) {
        self.edited_at = Some(Instant::now());
    }

    /// Write to a temp file next to the target, then rename over it, so a
    /// crash mid-write never leaves a truncated file.
    pub fn save(&mut self) {
        // Rename onto the symlink target, not the link, or the link becomes a plain file.
        let target = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let tmp = target.with_extension("md.mdeer-tmp");
        let result = std::fs::write(&tmp, &self.text).and_then(|()| std::fs::rename(&tmp, &target));
        match result {
            Ok(()) => {
                self.disk = stamp(&self.path);
                self.edited_at = None;
                self.conflict = false;
                self.error = None;
            }
            // Keep `edited_at` so the next tick retries; the edits stay in memory.
            Err(e) => self.error = Some(format!("Could not save: {e}")),
        }
    }

    /// Save pending edits once they are `AUTOSAVE_DELAY` old, or right away if `now` is true.
    pub fn autosave(&mut self, now: bool) {
        let Some(t) = self.edited_at else { return };
        if self.conflict || !(now || t.elapsed() >= AUTOSAVE_DELAY) {
            return;
        }
        if stamp(&self.path) != self.disk {
            self.conflict = true;
            return;
        }
        self.save();
    }

    /// Pick up writes by other programs: reload if we have no edits, else flag a conflict.
    pub fn sync_from_disk(&mut self) {
        if stamp(&self.path) == self.disk {
            return;
        }
        if self.edited_at.is_some() {
            self.conflict = true;
        } else {
            self.reload();
        }
    }

    pub fn reload(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                self.text = text;
                self.disk = stamp(&self.path);
                self.edited_at = None;
                self.conflict = false;
                self.error = None;
                self.cache.clear_viewer(&self.id());
            }
            Err(e) => self.error = Some(format!("Could not reload: {e}")),
        }
    }

    pub fn toggle_editing(&mut self) {
        self.editing = !self.editing;
        if !self.editing {
            // The viewport cache doesn't notice text changes on its own.
            self.cache.clear_viewer(&self.id());
        }
    }

    /// Small files get clickable task checkboxes; large ones the virtualized, read-only viewer.
    // ponytail: egui_commonmark can't mutate and virtualize at once; drop the split when it can.
    pub fn reading_view(&mut self, ui: &mut egui::Ui) {
        let viewer = CommonMarkViewer::new()
            .default_implicit_uri_scheme(&self.image_base)
            .enable_scroll_to_heading(true);
        if self.text.len() > CHECKBOX_MAX_BYTES {
            viewer.show_scrollable(
                self.id(),
                ui,
                &mut self.cache,
                &CommonMarkScrollOptions::default().viewport_cache(true),
                &self.text,
            );
            return;
        }
        let changed = egui::ScrollArea::vertical()
            .id_salt(self.id())
            .show(ui, |ui| {
                viewer
                    .show_mut(ui, &mut self.cache, &mut self.text)
                    .response
                    .changed()
            })
            .inner;
        if changed {
            self.mark_edited();
        }
    }

    pub fn source_view(&mut self, ui: &mut egui::Ui, focus: bool) {
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
        let changed = egui::ScrollArea::vertical()
            .show(ui, |ui| {
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
                response.changed()
            })
            .inner;
        if changed {
            self.mark_edited();
        }
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
        self.mark_edited();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_doc(name: &str, text: &str) -> Doc {
        let dir = std::env::temp_dir().join("mdeer-test").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("doc.md"), text).unwrap();
        Doc::open(&dir.join("doc.md")).unwrap()
    }

    /// Simulates another program writing the file (different length, so the stamp changes
    /// even on filesystems with coarse mtimes).
    fn external_write(doc: &Doc, text: &str) {
        std::fs::write(&doc.path, text).unwrap();
    }

    #[test]
    fn autosave_waits_for_delay_then_writes() {
        let mut doc = temp_doc("delay", "a");
        doc.text.push('b');
        doc.mark_edited();
        doc.autosave(false);
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), "a");
        doc.autosave(true);
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), "ab");
        assert!(doc.edited_at.is_none());
        assert!(!doc.path.with_extension("md.mdeer-tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn save_through_symlink_keeps_the_link() {
        let mut doc = temp_doc("symlink", "a");
        let link = doc.path.with_file_name("link.md");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&doc.path, &link).unwrap();
        let target = std::mem::replace(&mut doc.path, link.clone());
        doc.text = "b".into();
        doc.save();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "b");
    }

    #[test]
    fn external_change_reloads_when_clean() {
        let mut doc = temp_doc("clean", "a");
        external_write(&doc, "changed");
        doc.sync_from_disk();
        assert_eq!(doc.text, "changed");
        assert!(!doc.conflict);
    }

    #[test]
    fn external_change_with_edits_is_a_conflict_and_keeps_local_text() {
        let mut doc = temp_doc("conflict", "a");
        doc.text = "mine".into();
        doc.mark_edited();
        external_write(&doc, "theirs!");
        doc.autosave(true);
        assert!(doc.conflict);
        assert_eq!(doc.text, "mine");
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), "theirs!");
        // "Keep mine" overwrites.
        doc.save();
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), "mine");
    }

    #[test]
    fn relative_image_loads_next_to_file() {
        let doc = temp_doc("image", "![a](img/a.png)");
        let dir = doc.path.parent().unwrap();
        std::fs::create_dir_all(dir.join("img")).unwrap();
        let mut png = Vec::new();
        image::RgbImage::new(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.join("img/a.png"), png).unwrap();

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
