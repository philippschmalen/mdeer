//! Source view: markdown syntax highlighting and formatting shortcuts.

use std::ops::Range;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{FontId, Stroke};
use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::theme;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Heading,
    Emphasis,
    Strong,
    Strike,
    Code,
    Link,
    Quote,
    Muted,
}

pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

/// One kind per byte; later (inner) spans overwrite earlier (outer) ones.
fn classify(text: &str) -> Vec<Kind> {
    let mut kinds = vec![Kind::Text; text.len()];
    let mut paint = |r: Range<usize>, k| kinds[r].fill(k);
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) => paint(range, Kind::Heading),
            Event::Start(Tag::Emphasis) => paint(range, Kind::Emphasis),
            Event::Start(Tag::Strong) => paint(range, Kind::Strong),
            Event::Start(Tag::Strikethrough) => paint(range, Kind::Strike),
            Event::Start(Tag::BlockQuote(_)) => paint(range, Kind::Quote),
            Event::Start(Tag::CodeBlock(_)) | Event::Code(_) => paint(range, Kind::Code),
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => paint(range, Kind::Link),
            Event::Html(_) | Event::InlineHtml(_) | Event::Rule => paint(range, Kind::Muted),
            _ => {}
        }
    }
    kinds
}

fn format(kind: Kind, font_id: &FontId) -> TextFormat {
    let mut f = TextFormat::simple(font_id.clone(), theme::FG);
    match kind {
        Kind::Text => {}
        Kind::Heading => f.color = theme::BLUE,
        Kind::Emphasis => f.italics = true,
        Kind::Strong => f.color = theme::ORANGE,
        Kind::Strike => f.strikethrough = Stroke::new(1.0, theme::COMMENT),
        Kind::Code => f.color = theme::GREEN,
        Kind::Link => f.color = theme::CYAN,
        Kind::Quote => (f.color, f.italics) = (theme::COMMENT, true),
        Kind::Muted => f.color = theme::COMMENT,
    }
    f
}

pub fn highlight(text: &str, font_id: &FontId) -> LayoutJob {
    let kinds = classify(text);
    let mut job = LayoutJob::default();
    let mut start = 0;
    while start < text.len() {
        let kind = kinds[start];
        let len = kinds[start..].iter().take_while(|&&k| k == kind).count();
        // Ranges from the parser always land on char boundaries.
        job.append(&text[start..start + len], 0.0, format(kind, font_id));
        start += len;
    }
    job
}

/// Byte offset of the `char_idx`-th char.
fn byte_at(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map_or(text.len(), |(b, _)| b)
}

/// Wraps the selected chars `sel` in `before`/`after` and returns the new
/// char selection: the wrapped text, or the gap between the markers if `sel` was empty.
pub fn wrap(text: &mut String, sel: Range<usize>, before: &str, after: &str) -> Range<usize> {
    let (start, end) = (byte_at(text, sel.start), byte_at(text, sel.end));
    text.insert_str(end, after);
    text.insert_str(start, before);
    let shift = before.chars().count();
    sel.start + shift..sel.end + shift
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_spans_win() {
        let k = classify("# Hi **b**\n");
        assert!(k[..5].iter().all(|&k| k == Kind::Heading));
        assert!(k[5..10].iter().all(|&k| k == Kind::Strong));
    }

    #[test]
    fn layout_covers_all_text() {
        let text = "a *é* `c` [l](u)\n\n> q\n";
        let job = highlight(text, &FontId::monospace(12.0));
        assert_eq!(job.text, text);
    }

    #[test]
    fn wrap_selection_and_empty() {
        let mut t = String::from("é bold");
        assert_eq!(wrap(&mut t, 2..6, "**", "**"), 4..8);
        assert_eq!(t, "é **bold**");
        let mut t = String::from("ab");
        assert_eq!(wrap(&mut t, 1..1, "*", "*"), 2..2);
        assert_eq!(t, "a**b");
    }
}
