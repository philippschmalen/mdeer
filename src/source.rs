//! Source view: markdown syntax highlighting, formatting shortcuts and typing helpers.

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

/// A keystroke the source view may handle itself (Obsidian-style typing helpers).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Input {
    Enter,
    Char(char),
    Backspace,
    Indent,
    Outdent,
}

const PAIRS: [(char, char); 5] = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('`', '`')];
/// Typed over a selection, these wrap it (closed by their pair, or by themselves).
const WRAPPERS: &str = "([{\"`*_~=";

fn closer(open: char) -> Option<char> {
    PAIRS.iter().find(|p| p.0 == open).map(|p| p.1)
}

/// Char cursor at byte offset `b`.
fn caret(text: &str, b: usize) -> Range<usize> {
    let c = text[..b].chars().count();
    c..c
}

/// `(indent, marker, next)` of a list item or quote line: leading whitespace,
/// the marker with its trailing space, and the marker a continuation line gets.
fn marker(line: &str) -> Option<(&str, &str, String)> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - rest.len()];
    let b = rest.as_bytes();
    let (len, next) = if rest.starts_with("> ") {
        (2, "> ".to_owned())
    } else if matches!(b, [b'-' | b'*' | b'+', b' ', ..]) {
        let bullet = &rest[..2];
        if ["[ ] ", "[x] ", "[X] "]
            .iter()
            .any(|t| rest[2..].starts_with(t))
        {
            (6, format!("{bullet}[ ] "))
        } else {
            (2, bullet.to_owned())
        }
    } else {
        let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
        let [delim @ (b'.' | b')'), b' '] = b.get(digits..digits + 2)? else {
            return None;
        };
        let n: u64 = rest[..digits].parse().ok()?;
        (digits + 2, format!("{}{} ", n + 1, *delim as char))
    };
    Some((indent, &rest[..len], next))
}

/// Applies a typing helper to the char selection `sel` and returns the new selection,
/// or `None` to leave the keystroke to the plain text editor.
pub fn edit(text: &mut String, sel: Range<usize>, input: Input) -> Option<Range<usize>> {
    let (start, end) = (byte_at(text, sel.start), byte_at(text, sel.end));
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let line = &text[line_start..line_end];
    let prev = text[..start].chars().next_back();
    let next = text[end..].chars().next();
    let after = sel.start + 1..sel.start + 1;
    match input {
        Input::Char(c) if start != end => {
            let close = closer(c).unwrap_or(c);
            WRAPPERS
                .contains(c)
                .then(|| wrap(text, sel, &c.to_string(), &close.to_string()))
        }
        Input::Indent | Input::Outdent if end <= line_end => {
            marker(line)?;
            if input == Input::Indent {
                text.insert(line_start, '\t');
                return Some(sel.start + 1..sel.end + 1);
            }
            let n = if line.starts_with('\t') {
                1
            } else {
                line.bytes().take(4).take_while(|&b| b == b' ').count()
            };
            text.replace_range(line_start..line_start + n, "");
            let home = caret(text, line_start).start;
            Some(sel.start.saturating_sub(n).max(home)..sel.end.saturating_sub(n).max(home))
        }
        _ if start != end => None,
        Input::Enter => {
            let (indent, m, next_marker) = marker(line)?;
            let body = line_start + indent.len() + m.len();
            if start < body {
                return None;
            }
            if text[body..line_end].trim().is_empty() {
                // Enter on an empty item ends the list.
                text.replace_range(line_start..line_end, "");
                return Some(caret(text, line_start));
            }
            let insert = format!("\n{indent}{next_marker}");
            text.insert_str(start, &insert);
            Some(caret(text, start + insert.len()))
        }
        Input::Char(c) if next == Some(c) && PAIRS.iter().any(|p| p.1 == c) => Some(after),
        Input::Char('`') if prev == Some('`') => {
            // Third backtick of an opening fence: add the closing one.
            let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
            let fences_above = text[..line_start]
                .lines()
                .filter(|l| l.trim_start().starts_with("```"))
                .count();
            if start != line_end
                || text[line_start..start] != format!("{indent}``")
                || fences_above % 2 == 1
            {
                return None;
            }
            let insert = format!("`\n{indent}```");
            text.insert_str(start, &insert);
            Some(after)
        }
        Input::Char(c) => {
            let close = closer(c)?;
            if c == '"' && prev.is_some_and(char::is_alphanumeric) {
                return None;
            }
            text.insert_str(start, &format!("{c}{close}"));
            Some(after)
        }
        Input::Backspace => {
            let (p, n) = (prev?, next?);
            if closer(p) != Some(n) {
                return None;
            }
            text.replace_range(start - p.len_utf8()..start + n.len_utf8(), "");
            Some(sel.start - 1..sel.start - 1)
        }
        Input::Indent | Input::Outdent => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_(text: &str, cursor: usize, input: Input) -> (String, Option<Range<usize>>) {
        let mut t = text.to_owned();
        let sel = edit(&mut t, cursor..cursor, input);
        (t, sel)
    }

    #[test]
    fn enter_continues_lists() {
        for (line, next) in [
            ("- a", "- "),
            ("* a", "* "),
            ("+ a", "+ "),
            ("9. a", "10. "),
            ("1) a", "2) "),
            ("- [x] a", "- [ ] "),
            ("> a", "> "),
            ("\t  - a", "\t  - "),
        ] {
            let n = line.chars().count();
            let (t, sel) = type_(line, n, Input::Enter);
            assert_eq!(t, format!("{line}\n{next}"));
            assert_eq!(sel, Some(t.chars().count()..t.chars().count()));
        }
        assert_eq!(type_("plain", 5, Input::Enter).1, None);
        assert_eq!(type_("-not", 4, Input::Enter).1, None);
        assert_eq!(type_("2024.", 5, Input::Enter).1, None);
    }

    #[test]
    fn enter_mid_line_splits_and_empty_item_exits() {
        assert_eq!(
            type_("- foobar", 5, Input::Enter),
            ("- foo\n- bar".into(), Some(8..8))
        );
        assert_eq!(
            type_("x\n- [ ] ", 8, Input::Enter),
            ("x\n".into(), Some(2..2))
        );
        // Cursor before the marker: plain newline.
        assert_eq!(type_("- a", 0, Input::Enter).1, None);
    }

    #[test]
    fn tab_indents_list_lines_only() {
        assert_eq!(
            type_("x\n- a", 4, Input::Indent),
            ("x\n\t- a".into(), Some(5..5))
        );
        assert_eq!(
            type_("\t- a", 3, Input::Outdent),
            ("- a".into(), Some(2..2))
        );
        assert_eq!(
            type_("  - a", 1, Input::Outdent),
            ("- a".into(), Some(0..0))
        );
        assert_eq!(type_("plain", 2, Input::Indent).1, None);
    }

    #[test]
    fn pairs_type_over_and_delete_together() {
        assert_eq!(type_("é", 1, Input::Char('(')), ("é()".into(), Some(2..2)));
        assert_eq!(type_("()", 1, Input::Char(')')), ("()".into(), Some(2..2)));
        assert_eq!(type_("[]", 1, Input::Backspace), ("".into(), Some(0..0)));
        assert_eq!(type_("[x", 1, Input::Backspace).1, None);
        assert_eq!(type_("say", 3, Input::Char('"')).1, None);
        assert_eq!(type_("a", 1, Input::Char('*')).1, None);
    }

    #[test]
    fn typed_char_wraps_selection() {
        let mut t = String::from("a b");
        assert_eq!(edit(&mut t, 2..3, Input::Char('~')), Some(3..4));
        assert_eq!(t, "a ~b~");
        let mut t = String::from("b");
        assert_eq!(edit(&mut t, 0..1, Input::Char('(')), Some(1..2));
        assert_eq!(t, "(b)");
        assert_eq!(edit(&mut t, 0..1, Input::Char('x')), None);
    }

    #[test]
    fn backticks_build_a_fence() {
        let mut t = String::from("  ");
        let mut sel = 2..2;
        for _ in 0..3 {
            sel = match edit(&mut t, sel.clone(), Input::Char('`')) {
                Some(new) => new,
                None => {
                    t.insert(byte_at(&t, sel.start), '`');
                    sel.start + 1..sel.start + 1
                }
            };
        }
        assert_eq!((t.as_str(), sel), ("  ```\n  ```", 5..5));
        // Closing an open fence adds no extra one.
        assert_eq!(type_("```\nx\n``", 8, Input::Char('`')).1, None);
    }

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
