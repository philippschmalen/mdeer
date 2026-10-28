//! One open markdown file: its text, views and autosave state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use std::{io::Write, ops::Range};

use eframe::egui::{self, Event, Key, Modifiers};
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};

use crate::{pressed, source};

pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);
/// Above this, the reading view is virtualized and task checkboxes are read-only.
const CHECKBOX_MAX_BYTES: usize = 100_000;

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

const CODE_THEME: &str = "tokyo-night";
const YOURS: &str = " MDeer: yours";
const EXTERNAL: &str = " MDeer: external";

fn marker_length(line: &str, delimiter: char, label: &str) -> Option<usize> {
    let rest = line.trim_start_matches(delimiter);
    let length = line.len() - rest.len();
    (length >= 7 && rest == label).then_some(length)
}

pub enum Resolution {
    Mine,
    Disk,
    Both,
}

struct Conflict {
    whole: Range<usize>,
    mine: Range<usize>,
    disk: Range<usize>,
}

/// Only our labelled regions are actionable, not Markdown rules or Git examples.
fn conflicts(text: &str) -> Vec<Conflict> {
    let mut regions = Vec::new();
    let (mut start, mut middle) = (None, None);
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let label = line.trim_end_matches(['\r', '\n']);
        if let Some(length) = marker_length(label, '<', YOURS) {
            start = Some((offset, offset + line.len(), length));
            middle = None;
        } else if let Some((_, _, length)) = start
            && marker_length(label, '=', "") == Some(length)
            && middle.is_none()
        {
            middle = Some((offset, offset + line.len()));
        } else if let Some(length) = marker_length(label, '>', EXTERNAL) {
            if let (Some((begin, mine, opening)), Some((mine_end, disk))) = (start, middle)
                && opening == length
            {
                regions.push(Conflict {
                    whole: begin..offset + line.len(),
                    mine: mine..mine_end,
                    disk: disk..offset,
                });
            }
            (start, middle) = (None, None);
        }
        offset += line.len();
    }
    regions
}

fn merge(base: &str, local: &str, external: &str) -> String {
    // Choose a delimiter absent from the inputs so relabelling cannot change Git examples.
    let length = [base, local, external]
        .into_iter()
        .flat_map(str::lines)
        .map(|line| {
            let first = line.as_bytes().first().copied();
            line.bytes()
                .take_while(|b| Some(*b) == first && matches!(b, b'<' | b'>' | b'='))
                .count()
        })
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(7);
    let mut options = diffy::MergeOptions::new();
    options
        .set_conflict_style(diffy::ConflictStyle::Merge)
        .set_incomplete_hunk_style(diffy::IncompleteHunkStyle::Git)
        .set_conflict_marker_length(length);
    match options.merge(base, local, external) {
        Ok(text) => text,
        Err(text) => {
            let ours = format!("{} ours", "<".repeat(length));
            let theirs = format!("{} theirs", ">".repeat(length));
            let ours_label = format!("{}{YOURS}", "<".repeat(length));
            let theirs_label = format!("{}{EXTERNAL}", ">".repeat(length));
            let mut labelled = String::with_capacity(text.len());
            for line in text.split_inclusive('\n') {
                let label = line.trim_end_matches('\n');
                let replacement = if label == ours {
                    Some(ours_label.as_str())
                } else if label == theirs {
                    Some(theirs_label.as_str())
                } else {
                    None
                };
                if let Some(replacement) = replacement {
                    labelled.push_str(replacement);
                    if line.ends_with('\n') {
                        labelled.push('\n');
                    }
                } else {
                    labelled.push_str(line);
                }
            }
            labelled
        }
    }
}

fn replacement_path(target: &Path, attempt: usize) -> PathBuf {
    target.with_file_name(format!(".mdeer-{}-{attempt}.tmp", std::process::id()))
}

fn markdown_cache() -> CommonMarkCache {
    let mut cache = CommonMarkCache::default();
    cache
        .add_syntax_theme_from_bytes(CODE_THEME, include_bytes!("../assets/tokyo-night.tmTheme"))
        .expect("bundled theme parses");
    cache
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
    /// Last accepted disk text, including an external snapshot merged but not yet saved.
    base: String,
    /// A further external change cannot be merged until the pending markers are resolved.
    pub merge_paused: bool,
    reset_editor: bool,
    pub error: Option<String>,
}

impl Doc {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        // `absolute`, not `canonicalize`: Windows verbatim `\\?\` paths break `file://` image URIs.
        let path = std::path::absolute(path)?;
        let text = std::fs::read_to_string(&path)?;
        let editing = Self::contains_conflicts(&text);
        Ok(Self {
            image_base: file_url(path.parent().unwrap_or(Path::new("."))) + "/",
            disk: stamp(&path),
            path,
            base: text.clone(),
            text,
            cache: markdown_cache(),
            editing,
            highlight: Default::default(),
            edited_at: None,
            merge_paused: false,
            reset_editor: false,
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

    fn contains_conflicts(text: &str) -> bool {
        text.lines().any(|line| {
            marker_length(line, '<', YOURS).is_some()
                || marker_length(line, '>', EXTERNAL).is_some()
        })
    }

    pub fn has_conflicts(&self) -> bool {
        Self::contains_conflicts(&self.text)
    }

    pub fn conflict_count(&self) -> usize {
        conflicts(&self.text).len()
    }

    fn replace_text(&mut self, text: String) {
        self.text = text;
        self.reset_editor = true;
        self.highlight = Default::default();
        self.cache.clear_viewers();
        if self.has_conflicts() {
            self.editing = true;
        }
    }

    pub fn resolve_first(&mut self, resolution: Resolution) {
        // Reparse at click time: manual edits may have moved or removed a region.
        let Some(region) = conflicts(&self.text).into_iter().next() else {
            return;
        };
        let replacement = match resolution {
            Resolution::Mine => self.text[region.mine].to_owned(),
            Resolution::Disk => self.text[region.disk].to_owned(),
            Resolution::Both => format!("{}{}", &self.text[region.mine], &self.text[region.disk]),
        };
        let mut text = self.text.clone();
        text.replace_range(region.whole, &replacement);
        self.replace_text(text);
        self.mark_edited();
    }

    fn reconcile(&mut self) -> std::io::Result<()> {
        let external = std::fs::read_to_string(&self.path)?;
        if external != self.base {
            if self.edited_at.is_none() {
                self.replace_text(external.clone());
            } else if self.has_conflicts() {
                self.merge_paused = true;
                return Err(std::io::Error::other(
                    "Disk changed again. Resolve the MDeer markers before merging; \
                     disk and pending edits have been preserved.",
                ));
            } else {
                self.replace_text(merge(&self.base, &self.text, &external));
            }
            // Advance even if replacement fails later, or a retry would merge the same edits twice.
            self.base = external;
        }
        self.disk = stamp(&self.path);
        self.merge_paused = false;
        self.error = None;
        Ok(())
    }

    /// Write to a temp file next to the target, then rename over it, so a
    /// crash mid-write never leaves a truncated file.
    pub fn save(&mut self) -> bool {
        if self.text != self.base && self.edited_at.is_none() {
            self.mark_edited();
        }
        let result = self.reconcile().and_then(|()| {
            if self.edited_at.is_none() {
                return Ok(());
            }
            self.write_pending()
        });
        match result {
            Ok(()) => {
                self.base.clone_from(&self.text);
                self.disk = stamp(&self.path);
                self.edited_at = None;
                self.error = None;
                true
            }
            // Keep `edited_at` so the next tick retries; the edits stay in memory.
            Err(e) => {
                self.error = Some(format!("Could not save: {e}"));
                false
            }
        }
    }

    fn write_pending(&self) -> std::io::Result<()> {
        // Rename onto the symlink target, not the link, or the link becomes a plain file.
        let target = std::fs::canonicalize(&self.path)?;
        for attempt in 0..32 {
            let tmp = replacement_path(&target, attempt);
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp);
            let mut file = match file {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            };
            let result = file.write_all(self.text.as_bytes());
            drop(file);
            let result = result.and_then(|()| {
                if std::fs::read_to_string(&self.path)? != self.base
                    || std::fs::canonicalize(&self.path)? != target
                {
                    return Err(std::io::Error::other(
                        "File changed again; save will retry.",
                    ));
                }
                std::fs::rename(&tmp, &target)
            });
            if result.is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
            return result;
        }
        Err(std::io::Error::other("No unused same-directory save file."))
    }

    /// Save pending edits once they are `AUTOSAVE_DELAY` old, or right away if `now` is true.
    pub fn autosave(&mut self, now: bool) -> bool {
        if now
            || self
                .edited_at
                .is_some_and(|t| t.elapsed() >= AUTOSAVE_DELAY)
        {
            self.save()
        } else {
            true
        }
    }

    /// Pick up external writes, retaining the accepted disk snapshot as the merge base.
    pub fn sync_from_disk(&mut self) {
        if stamp(&self.path) == self.disk && !self.merge_paused && self.error.is_none() {
            return;
        }
        if let Err(e) = self.reconcile() {
            self.error = Some(format!("Could not reconcile: {e}"));
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
            .syntax_theme_dark(CODE_THEME)
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
        if self.reset_editor {
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let n = self.text.chars().count();
                if let Some(range) = state.cursor.char_range() {
                    state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(range.primary.index.0.min(n)),
                        CCursor::new(range.secondary.index.0.min(n)),
                    )));
                }
                state.clear_undoer();
                state.store(ui.ctx(), id);
            }
            self.reset_editor = false;
        }
        for (key, before, after) in [
            (Key::B, "**", "**"),
            (Key::I, "*", "*"),
            (Key::K, "[", "]()"),
        ] {
            if pressed(ui, key) {
                self.format_selection(ui.ctx(), id, before, after);
            }
        }
        self.typing_helpers(ui, id);

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

    /// Obsidian-style list continuation and auto-pairing: handles the keystroke events
    /// `source::edit` claims and removes them before the `TextEdit` sees them.
    fn typing_helpers(&mut self, ui: &egui::Ui, id: egui::Id) {
        if !ui.memory(|m| m.has_focus(id)) {
            return;
        }
        let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
            return;
        };
        let Some(range) = state.cursor.char_range() else {
            return;
        };
        let sorted = range.as_sorted_char_range();
        let mut sel = sorted.start.0..sorted.end.0;
        let mut changed = false;
        // ponytail: stop at the first edit we leave to the TextEdit so events keep their
        // order; a helper may be skipped when several keystrokes land in one frame.
        let mut stop = false;
        ui.input_mut(|i| {
            i.events.retain(|event| {
                if stop {
                    return true;
                }
                let input = match event {
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => match (key, *modifiers) {
                        (Key::Enter, Modifiers::NONE) => source::Input::Enter,
                        (Key::Backspace, Modifiers::NONE) => source::Input::Backspace,
                        (Key::Tab, Modifiers::NONE) => source::Input::Indent,
                        (Key::Tab, Modifiers::SHIFT) => source::Input::Outdent,
                        (
                            Key::Enter
                            | Key::Backspace
                            | Key::Tab
                            | Key::Delete
                            | Key::Home
                            | Key::End
                            | Key::PageUp
                            | Key::PageDown
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::ArrowUp
                            | Key::ArrowDown,
                            _,
                        ) => {
                            stop = true;
                            return true;
                        }
                        _ => {
                            stop |= modifiers.command;
                            return true;
                        }
                    },
                    Event::Text(t) if t.chars().count() == 1 => {
                        source::Input::Char(t.chars().next().unwrap_or_default())
                    }
                    Event::Text(_) | Event::Paste(_) | Event::Cut => {
                        stop = true;
                        return true;
                    }
                    _ => return true,
                };
                match source::edit(&mut self.text, sel.clone(), input) {
                    Some(new) => {
                        (sel, changed) = (new, true);
                        false
                    }
                    None => {
                        stop = true;
                        true
                    }
                }
            });
        });
        if changed {
            let (a, b) = (CCursor::new(sel.start), CCursor::new(sel.end));
            state.cursor.set_char_range(Some(CCursorRange::two(a, b)));
            state.store(ui.ctx(), id);
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

    struct TestDoc {
        doc: Doc,
        dir: PathBuf,
    }

    impl std::ops::Deref for TestDoc {
        type Target = Doc;
        fn deref(&self) -> &Doc {
            &self.doc
        }
    }

    impl std::ops::DerefMut for TestDoc {
        fn deref_mut(&mut self) -> &mut Doc {
            &mut self.doc
        }
    }

    impl Drop for TestDoc {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).unwrap();
        }
    }

    fn temp_doc(name: &str, text: &str) -> TestDoc {
        let dir = std::env::current_dir()
            .unwrap()
            .join(".test-artifacts")
            .join(format!("doc-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("doc.md"), text).unwrap();
        TestDoc {
            doc: Doc::open(&dir.join("doc.md")).unwrap(),
            dir,
        }
    }

    /// Simulates another program writing the file (different length, so the stamp changes
    /// even on filesystems with coarse mtimes).
    fn external_write(doc: &Doc, text: &str) {
        std::fs::write(&doc.path, text).unwrap();
    }

    #[test]
    fn typing_helpers_reach_the_editor() {
        let mut doc = temp_doc("typing", "- a");
        let ctx = egui::Context::default();
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![],
            vec![key(Key::End)],
            vec![key(Key::Enter), Event::Text("(".into())],
            vec![Event::Text("b".into())],
        ] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| doc.source_view(ui, true))
                .textures_delta
                .clear();
        }
        assert_eq!(doc.text, "- a\n- (b)");
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
        assert!(!doc.has_conflicts());
    }

    #[test]
    fn conflicts_are_saved_reopened_and_resolved_one_region_at_a_time() {
        let mut doc = temp_doc("conflict", "a");
        doc.text = "mine".into();
        doc.mark_edited();
        external_write(&doc, "theirs!");
        assert!(doc.autosave(true));
        assert_eq!(doc.conflict_count(), 1);
        assert!(doc.edited_at.is_none());
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), doc.text);
        let reopened = Doc::open(&doc.path).unwrap();
        assert!(reopened.editing);
        assert!(reopened.has_conflicts());
        assert!(doc.autosave(true), "saved conflicts do not block close");
        doc.resolve_first(Resolution::Both);
        assert!(!doc.has_conflicts());
        assert!(doc.autosave(true));
        assert_eq!(
            std::fs::read_to_string(&doc.path).unwrap(),
            "mine\ntheirs!\n"
        );
    }

    #[test]
    fn disjoint_changes_merge_without_duplicating_on_save_retry() {
        let mut doc = temp_doc("merge-retry", "first\nmiddle\nlast\n");
        doc.text = "mine\nmiddle\nlast\n".into();
        doc.mark_edited();
        external_write(&doc, "first\nmiddle\nexternal\n");
        // Occupied replacement names force a portable write failure, even as root.
        for attempt in 0..32 {
            std::fs::create_dir(replacement_path(&doc.path, attempt)).unwrap();
        }
        assert!(!doc.autosave(true));
        assert_eq!(doc.base, "first\nmiddle\nexternal\n");
        assert_eq!(doc.text, "mine\nmiddle\nexternal\n");
        assert!(doc.edited_at.is_some());
        assert!(doc.error.is_some());
        assert_eq!(
            std::fs::read_to_string(&doc.path).unwrap(),
            "first\nmiddle\nexternal\n"
        );
        for attempt in 0..32 {
            std::fs::remove_dir(replacement_path(&doc.path, attempt)).unwrap();
        }
        assert!(doc.autosave(true));
        assert_eq!(doc.text, "mine\nmiddle\nexternal\n");
        assert_eq!(doc.base, doc.text);
        assert!(doc.edited_at.is_none());
    }

    #[test]
    fn a_failed_conflict_write_retries_without_nesting_markers() {
        let mut doc = temp_doc("conflict-retry", "base");
        doc.text = "mine".into();
        doc.mark_edited();
        external_write(&doc, "external");
        for attempt in 0..32 {
            std::fs::create_dir(replacement_path(&doc.path, attempt)).unwrap();
        }
        assert!(!doc.autosave(true));
        let pending = doc.text.clone();
        assert_eq!(doc.base, "external");
        assert_eq!(doc.conflict_count(), 1);
        assert!(!doc.merge_paused);
        assert!(doc.edited_at.is_some());
        for attempt in 0..32 {
            std::fs::remove_dir(replacement_path(&doc.path, attempt)).unwrap();
        }
        assert!(doc.autosave(true));
        assert_eq!(doc.text, pending);
        assert_eq!(doc.conflict_count(), 1);
        assert!(doc.edited_at.is_none());
    }

    #[test]
    fn further_external_changes_pause_until_manual_resolution() {
        let mut doc = temp_doc("pause", "first\nmiddle\nlast\nend\n");
        doc.text = "mine\nmiddle\nlast\nend\n".into();
        doc.mark_edited();
        external_write(&doc, "disk\nmiddle\nlast\nend\n");
        assert!(doc.autosave(true));
        let marked = doc.text.clone();
        doc.text.push_str("local tail\n");
        doc.mark_edited();
        external_write(&doc, &marked.replace("last", "latest disk"));
        let pending = doc.text.clone();
        for _ in 0..2 {
            assert!(!doc.autosave(true));
            assert!(doc.merge_paused);
            assert_eq!(doc.text, pending);
            assert_eq!(doc.conflict_count(), 1);
        }
        // Manual edits are discovered without a cached conflict flag or stale offsets.
        let region = conflicts(&doc.text).remove(0);
        doc.text.replace_range(region.whole, "resolved\n");
        doc.mark_edited();
        assert!(doc.autosave(true));
        assert!(!doc.merge_paused);
        assert_eq!(doc.text, "resolved\nmiddle\nlatest disk\nend\nlocal tail\n");
    }

    #[test]
    fn clean_markers_can_reload_and_generic_git_examples_are_not_ours() {
        let example = "<<<<<<< ours\none\n=======\ntwo\n>>>>>>> theirs\n";
        let mut doc = temp_doc("generic", example);
        assert!(!doc.editing);
        assert!(!doc.has_conflicts());
        doc.text.push_str("local\n");
        doc.mark_edited();
        external_write(&doc, &format!("external\n{example}"));
        assert!(doc.autosave(true));
        assert_eq!(doc.text, format!("external\n{example}local\n"));
        assert!(!doc.has_conflicts());
        external_write(
            &doc,
            &format!("<<<<<<<{YOURS}\na\n=======\nb\n>>>>>>>{EXTERNAL}\n"),
        );
        doc.sync_from_disk();
        assert!(doc.has_conflicts());
        assert!(doc.editing);
        external_write(&doc, "clean replacement");
        doc.sync_from_disk();
        assert_eq!(doc.text, "clean replacement");
        assert!(!doc.has_conflicts());
    }

    #[test]
    fn resolutions_reparse_moved_unicode_crlf_regions_and_preserve_others() {
        let mut doc = temp_doc("regions", "α\r\nmiddle\r\nω");
        doc.text = "mine α\r\nmiddle\r\nmine ω".into();
        doc.mark_edited();
        external_write(&doc, "disk α\r\nmiddle\r\ndisk ω");
        assert!(doc.autosave(true));
        assert_eq!(doc.conflict_count(), 2);
        assert!(!doc.text.contains("|||||||"), "use Merge, not Diff3");
        doc.text.insert_str(0, "前書き\r\n");
        doc.mark_edited();
        let second = conflicts(&doc.text)[1].whole.clone();
        let second_text = doc.text[second].to_owned();
        doc.resolve_first(Resolution::Mine);
        assert_eq!(doc.conflict_count(), 1);
        assert!(doc.text.starts_with("前書き\r\nmine α\r\nmiddle\r\n"));
        assert!(doc.text.ends_with(&second_text));
        doc.resolve_first(Resolution::Disk);
        assert!(!doc.has_conflicts());
        assert_eq!(doc.text, "前書き\r\nmine α\r\nmiddle\r\ndisk ω\n");
        assert!(doc.autosave(true));
    }

    #[test]
    fn failed_disk_read_never_becomes_an_empty_merge_base() {
        let mut doc = temp_doc("read-error", "original");
        doc.text = "pending".into();
        doc.mark_edited();
        std::fs::remove_file(&doc.path).unwrap();
        std::fs::create_dir(&doc.path).unwrap();
        assert!(!doc.autosave(true));
        assert_eq!(doc.base, "original");
        assert_eq!(doc.text, "pending");
        assert!(doc.edited_at.is_some());
        std::fs::remove_dir(&doc.path).unwrap();
        external_write(&doc, "original");
        assert!(doc.autosave(true));
        assert_eq!(std::fs::read_to_string(&doc.path).unwrap(), "pending");
    }

    #[test]
    fn a_change_after_reconciliation_is_rechecked_before_replacement() {
        let mut doc = temp_doc("recheck", "original");
        doc.text = "pending".into();
        doc.mark_edited();
        doc.reconcile().unwrap();
        external_write(&doc, "new intervening text");
        assert!(doc.write_pending().is_err());
        assert_eq!(
            std::fs::read_to_string(&doc.path).unwrap(),
            "new intervening text"
        );
        assert!(doc.edited_at.is_some());
        assert!(!replacement_path(&doc.path, 0).exists());
        assert!(doc.autosave(true));
        assert_eq!(doc.conflict_count(), 1);
    }

    #[test]
    fn generated_labels_do_not_relabel_existing_git_examples() {
        let example = "<<<<<<< ours\none\n=======\ntwo\n>>>>>>> theirs\n\n";
        let mut doc = temp_doc("labels", &format!("{example}base"));
        doc.text = format!("{example}mine");
        doc.mark_edited();
        external_write(&doc, &format!("{example}disk"));
        assert!(doc.autosave(true));
        assert!(doc.text.starts_with(example));
        assert_eq!(doc.conflict_count(), 1);
        doc.resolve_first(Resolution::Both);
        assert_eq!(doc.text, format!("{example}mine\ndisk\n"));
        assert!(!doc.has_conflicts());
    }

    #[test]
    fn markdown_rules_inside_variants_are_not_region_separators() {
        let mut doc = temp_doc("variant-rule", "old\nbase\n");
        doc.text = "mine\n=======\nlocal\n".into();
        doc.mark_edited();
        external_write(&doc, "external\nother\n");
        assert!(doc.autosave(true));
        assert_eq!(doc.conflict_count(), 1);
        let mut reopened = Doc::open(&doc.path).unwrap();
        assert!(reopened.editing);
        reopened.resolve_first(Resolution::Disk);
        assert_eq!(reopened.text, "external\nother\n");
        doc.resolve_first(Resolution::Mine);
        assert_eq!(doc.text, "mine\n=======\nlocal\n");
        assert!(!doc.has_conflicts());
    }

    #[test]
    fn programmatic_replacement_clamps_char_cursors_and_clears_old_undo() {
        let mut doc = temp_doc("cursor", "old discarded text");
        let ctx = egui::Context::default();
        for events in [vec![], vec![Event::Text("typed".into())]] {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| doc.source_view(ui, true),
            )
            .textures_delta
            .clear();
        }
        let id = doc.id().with("source");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(100), CCursor::new(99))));
        state.store(&ctx, id);
        doc.edited_at = None;
        external_write(&doc, "é🌲");
        doc.sync_from_disk();
        ctx.run_ui(Default::default(), |ui| doc.source_view(ui, false))
            .textures_delta
            .clear();
        let range = egui::TextEdit::load_state(&ctx, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.primary.index.0, 2);
        assert_eq!(range.secondary.index.0, 2);
        ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Key {
                    key: Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ui| doc.source_view(ui, false),
        )
        .textures_delta
        .clear();
        assert_eq!(doc.text, "é🌲");
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

    /// Perf smoke check against the targets (startup <= 1 s, reaction < 50 ms), CPU side only.
    /// Not a CI gate (runners are noisy): `cargo test --release -- --ignored --nocapture perf`
    #[test]
    #[ignore]
    fn perf_smoke() {
        let block = include_str!("../AGENTS.md");
        for size in [CHECKBOX_MAX_BYTES, 1_000_000] {
            let mut doc = temp_doc(&format!("perf-{size}"), &block.repeat(size / block.len()));
            let ctx = egui::Context::default();
            crate::theme::apply(&ctx);
            let input = |events| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            let frame = |doc: &mut Doc, events, focus| {
                let t = Instant::now();
                let out = ctx.run_ui(input(events), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        if doc.editing {
                            doc.source_view(ui, focus);
                        } else {
                            doc.reading_view(ui);
                        }
                    });
                });
                let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
                t.elapsed()
            };
            let first = frame(&mut doc, vec![], false);
            let reading = frame(&mut doc, vec![], false);
            doc.toggle_editing();
            let source_first = frame(&mut doc, vec![], true);
            let len = doc.text.len();
            let keystroke = frame(&mut doc, vec![egui::Event::Text("x".into())], false);
            assert_eq!(doc.text.len(), len + 1, "keystroke reached the editor");
            eprintln!(
                "{size:>8} B: first frame {first:?}, reading frame {reading:?}, \
                 source first frame {source_first:?}, keystroke {keystroke:?}"
            );
            assert!(first < Duration::from_secs(1));
            assert!(reading < Duration::from_millis(50));
            assert!(keystroke < Duration::from_millis(50));
        }
    }
}
