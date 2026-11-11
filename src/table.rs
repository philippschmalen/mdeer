//! Table helpers: GFM pipe tables edited as plain text (ADR-0008).
//!
//! Every function takes the whole document and a char selection, rewrites at most the
//! one table it acts on, and returns the new char selection.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag};
use unicode_width::UnicodeWidthStr;

use crate::source::{byte_at, caret, marker, options};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Op {
    Next,
    Prev,
    NextRow,
    InsertRow { above: bool },
    InsertCol { left: bool },
    DeleteRow,
    DeleteCol,
    MoveRow { up: bool },
    MoveCol { left: bool },
    DeleteTable,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Align {
    None,
    Left,
    Center,
    Right,
}

struct Table {
    align: Vec<Align>,
    /// `rows[0]` is the header; cells keep their source text (escapes included).
    rows: Vec<Vec<String>>,
}

/// A table found around the cursor.
struct Found {
    table: Table,
    /// Bytes from the header's line start to past the last row's newline (if any).
    range: Range<usize>,
    eol: &'static str,
    row: usize,
    col: usize,
}

/// Splits on `|` outside backslash escapes (`\|`, `\\`), like cmark-gfm.
fn split(line: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let (mut start, mut escaped) = (0, false);
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '|' => {
                cells.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    cells.push(&line[start..]);
    cells
}

fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let mut cells = split(line);
    if line.starts_with('|') {
        cells.remove(0);
    }
    if cells.len() > 1 && cells.last() == Some(&"") || line == "|" {
        cells.pop();
    }
    cells.iter().map(|c| c.trim().to_owned()).collect()
}

fn separator(line: &str) -> Option<Vec<Align>> {
    if !line.contains('|') {
        return None;
    }
    cells(line)
        .iter()
        .map(|c| {
            let (l, r) = (c.starts_with(':'), c.len() > 1 && c.ends_with(':'));
            let dashes = &c[l as usize..c.len() - r as usize];
            (!dashes.is_empty() && dashes.bytes().all(|b| b == b'-')).then_some(match (l, r) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::None,
            })
        })
        .collect()
}

/// Content range of the line holding byte `b` (without `\n`, with any `\r`).
fn line_at(text: &str, b: usize) -> Range<usize> {
    let start = text[..b].rfind('\n').map_or(0, |i| i + 1);
    let end = text[b..].find('\n').map_or(text.len(), |i| b + i);
    start..end
}

fn content<'a>(text: &'a str, r: &Range<usize>) -> &'a str {
    let line = &text[r.clone()];
    line.strip_suffix('\r').unwrap_or(line)
}

// ponytail: tables inside list items or block quotes are not detected; strip and re-add
// the line prefix if that's ever wanted.
fn is_row(line: &str) -> bool {
    !line.trim().is_empty() && split(line).len() > 1 && marker(line).is_none()
}

/// Whether the byte span touches a fenced or indented code block.
fn in_code(text: &str, span: Range<usize>) -> bool {
    Parser::new_ext(text, options())
        .into_offset_iter()
        .any(|(event, r)| {
            matches!(event, Event::Start(Tag::CodeBlock(_)))
                && r.start <= span.end
                && (span.start < r.end || r.end == text.len())
        })
}

fn find(text: &str, b: usize) -> Option<Found> {
    let cur = line_at(text, b);
    if !is_row(content(text, &cur)) {
        return None;
    }
    let mut lines = vec![cur.clone()];
    while lines[0].start > 0 {
        let prev = line_at(text, lines[0].start - 1);
        if !is_row(content(text, &prev)) {
            break;
        }
        lines.insert(0, prev);
    }
    let k = lines.len() - 1;
    while lines.last().unwrap().end < text.len() {
        let next = line_at(text, lines.last().unwrap().end + 1);
        if !is_row(content(text, &next)) {
            break;
        }
        lines.push(next);
    }
    let h = (0..=k)
        .rev()
        .find(|&i| i + 1 < lines.len() && separator(content(text, &lines[i + 1])).is_some())?;
    if in_code(text, lines[h].start..lines.last().unwrap().end) {
        return None;
    }
    let lines = &lines[h..];
    let mut rows = vec![cells(content(text, &lines[0]))];
    rows.extend(lines[2..].iter().map(|l| cells(content(text, l))));
    let last = lines.last().unwrap().end;
    let eol = if text[lines[0].clone()].ends_with('\r') {
        "\r\n"
    } else {
        "\n"
    };
    let line_idx = k - h;
    let line = &text[cur.start..b];
    let pipes = split(line).len() - 1;
    let col = pipes - usize::from(pipes > 0 && line.trim_start().starts_with('|'));
    let mut table = Table {
        align: separator(content(text, &lines[1])).unwrap_or_default(),
        rows,
    };
    table.normalize();
    Some(Found {
        col: col.min(table.cols() - 1),
        row: line_idx.saturating_sub(1),
        range: lines[0].start..(last + 1).min(text.len()),
        eol,
        table,
    })
}

impl Table {
    fn cols(&self) -> usize {
        self.align.len()
    }

    /// Every row and the alignment get the widest row's cell count; nothing is dropped.
    fn normalize(&mut self) {
        let cols = self
            .rows
            .iter()
            .map(Vec::len)
            .chain([self.align.len(), 1])
            .max()
            .unwrap_or(1);
        self.align.resize(cols, Align::None);
        for row in &mut self.rows {
            row.resize(cols, String::new());
        }
    }

    fn empty(cols: usize, body: usize) -> Self {
        Self {
            align: vec![Align::None; cols],
            rows: vec![vec![String::new(); cols]; body + 1],
        }
    }

    /// The table's lines (no trailing newline) and, per row, the byte offset where
    /// each cell's text starts.
    fn render(&self, eol: &str) -> (String, Vec<Vec<usize>>) {
        let widths: Vec<usize> = (0..self.cols())
            .map(|c| {
                self.rows
                    .iter()
                    .map(|r| r[c].width())
                    .chain([3])
                    .max()
                    .unwrap_or(3)
            })
            .collect();
        let mut out = String::new();
        let mut starts = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push_str(eol);
            }
            let mut row_starts = Vec::new();
            out.push('|');
            for (cell, w) in row.iter().zip(&widths) {
                out.push(' ');
                row_starts.push(out.len());
                out.push_str(cell);
                out.push_str(&" ".repeat(w - cell.width() + 1));
                out.push('|');
            }
            starts.push(row_starts);
            if i == 0 {
                out.push_str(eol);
                out.push('|');
                for (a, w) in self.align.iter().zip(&widths) {
                    let (l, r) = match a {
                        Align::None => ("-", "-"),
                        Align::Left => (":", "-"),
                        Align::Center => (":", ":"),
                        Align::Right => ("-", ":"),
                    };
                    out.push_str(&format!(" {l}{}{r} |", "-".repeat(w - 2)));
                }
            }
        }
        (out, starts)
    }
}

enum Outcome {
    /// Rewrite the table and put the cursor at the end of this cell's text.
    Cell(usize, usize),
    /// Leave everything as it is (the key is still handled).
    Same,
    /// The last, empty row was removed: put the cursor on an empty line in its place.
    Exit,
    Remove,
}

impl Found {
    fn op(&mut self, op: Op) -> Outcome {
        let (row, col, cols) = (self.row, self.col, self.table.cols());
        let rows = &mut self.table.rows;
        let last = rows.len() - 1;
        let blank = || vec![String::new(); cols];
        match op {
            Op::Next if col + 1 < cols => Outcome::Cell(row, col + 1),
            Op::Next => {
                if row == last {
                    rows.push(blank());
                }
                Outcome::Cell(row + 1, 0)
            }
            Op::Prev if col > 0 => Outcome::Cell(row, col - 1),
            Op::Prev if row > 0 => Outcome::Cell(row - 1, cols - 1),
            Op::Prev => Outcome::Cell(0, 0),
            Op::NextRow if row > 0 && row == last && rows[row].iter().all(String::is_empty) => {
                rows.pop();
                Outcome::Exit
            }
            Op::NextRow | Op::InsertRow { above: false } => {
                if op == Op::NextRow && row < last {
                    return Outcome::Cell(row + 1, col);
                }
                rows.insert(row + 1, blank());
                Outcome::Cell(row + 1, col)
            }
            Op::InsertRow { above: true } => {
                let at = row.max(1);
                rows.insert(at, blank());
                Outcome::Cell(at, col)
            }
            Op::InsertCol { left } => {
                let at = col + usize::from(!left);
                self.table.align.insert(at, Align::None);
                for r in rows {
                    r.insert(at, String::new());
                }
                Outcome::Cell(row, at)
            }
            Op::DeleteRow if row == 0 => Outcome::Same,
            Op::DeleteRow => {
                rows.remove(row);
                Outcome::Cell(row.min(last - 1), col)
            }
            Op::DeleteCol if cols == 1 => Outcome::Remove,
            Op::DeleteCol => {
                self.table.align.remove(col);
                for r in rows {
                    r.remove(col);
                }
                Outcome::Cell(row, col.min(cols - 2))
            }
            Op::MoveRow { up: true } if row >= 2 => {
                rows.swap(row, row - 1);
                Outcome::Cell(row - 1, col)
            }
            Op::MoveRow { up: false } if row >= 1 && row < last => {
                rows.swap(row, row + 1);
                Outcome::Cell(row + 1, col)
            }
            Op::MoveCol { left: true } if col > 0 => {
                self.swap_cols(col, col - 1);
                Outcome::Cell(row, col - 1)
            }
            Op::MoveCol { left: false } if col + 1 < cols => {
                self.swap_cols(col, col + 1);
                Outcome::Cell(row, col + 1)
            }
            Op::MoveRow { .. } | Op::MoveCol { .. } => Outcome::Same,
            Op::DeleteTable => Outcome::Remove,
        }
    }

    fn swap_cols(&mut self, a: usize, b: usize) {
        self.table.align.swap(a, b);
        for r in &mut self.table.rows {
            r.swap(a, b);
        }
    }
}

/// Applies `op` to the table under the cursor and returns the new char selection,
/// or `None` if the cursor is not in a table.
pub fn apply(text: &mut String, sel: Range<usize>, op: Op) -> Option<Range<usize>> {
    let mut found = find(text, byte_at(text, sel.start))?;
    let range = found.range.clone();
    let tail = if text[range.clone()].ends_with('\n') {
        found.eol
    } else {
        ""
    };
    let cursor = match found.op(op) {
        Outcome::Same => return Some(sel),
        Outcome::Remove => {
            text.replace_range(range.clone(), "");
            range.start
        }
        Outcome::Exit => {
            let (lines, _) = found.table.render(found.eol);
            let new = format!("{lines}{}{tail}", found.eol);
            text.replace_range(range.clone(), &new);
            range.start + lines.len() + found.eol.len()
        }
        Outcome::Cell(row, col) => {
            let (lines, starts) = found.table.render(found.eol);
            let at = starts[row][col] + found.table.rows[row][col].len();
            text.replace_range(range.clone(), &format!("{lines}{tail}"));
            range.start + at
        }
    };
    Some(caret(text, cursor))
}

/// Whether the char position is inside a table the helpers act on.
pub fn at(text: &str, char_pos: usize) -> bool {
    find(text, byte_at(text, char_pos)).is_some()
}

/// Puts `block` on its own lines at the cursor line, with a blank line before and after.
/// Returns the byte offset where `block` starts.
fn place(text: &mut String, b: usize, block: &str, eol: &str) -> usize {
    let line = line_at(text, b);
    let blank = text[line.clone()].trim().is_empty();
    let (at, lead) = if blank {
        let prev_blank = line.start == 0 || text[line_at(text, line.start - 1)].trim().is_empty();
        (line.clone(), if prev_blank { "" } else { eol })
    } else {
        (
            line.end..line.end,
            if eol == "\n" { "\n\n" } else { "\r\n\r\n" },
        )
    };
    let next_blank = line.end < text.len() && text[line_at(text, line.end + 1)].trim().is_empty();
    let trail = if next_blank { "" } else { eol };
    text.replace_range(at.clone(), &format!("{lead}{block}{trail}"));
    at.start + lead.len()
}

fn eol(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Inserts an empty table with `cols` columns and `rows` body rows at the cursor line;
/// the cursor goes into the first header cell.
pub fn insert(text: &mut String, sel: Range<usize>, cols: usize, rows: usize) -> Range<usize> {
    let eol = eol(text);
    let (block, starts) = Table::empty(cols.max(1), rows).render(eol);
    let at = place(text, byte_at(text, sel.start), &block, eol);
    caret(text, at + starts[0][0])
}

/// Spreadsheet clipboard text (tab-separated, quoted cells allowed) as table rows:
/// at least 2×2 and every row the same width.
fn tsv(clip: &str) -> Option<Vec<Vec<String>>> {
    let (mut rows, mut row, mut cell) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut at_start) = (false, true);
    let mut chars = clip.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if at_start => quoted = true,
            '\r' if !quoted && chars.peek() == Some(&'\n') => {}
            '\t' | '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                if c == '\n' {
                    rows.push(std::mem::take(&mut row));
                }
                at_start = true;
                continue;
            }
            c => cell.push(c),
        }
        at_start = false;
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    let cols = rows.first()?.len();
    (rows.len() >= 2 && cols >= 2 && rows.iter().all(|r| r.len() == cols)).then_some(rows)
}

/// Pastes spreadsheet cells as a table on their own lines; `None` (paste as text) if
/// `clip` is not a grid or the cursor is in a table or code block.
pub fn paste(text: &mut String, sel: Range<usize>, clip: &str) -> Option<Range<usize>> {
    let rows = tsv(clip)?;
    let (start, end) = (byte_at(text, sel.start), byte_at(text, sel.end));
    if find(text, start).is_some() || in_code(text, start..start) {
        return None;
    }
    let rows = rows
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|c| {
                    c.trim()
                        .replace("\r\n", "<br>")
                        .replace('\n', "<br>")
                        .replace('|', "\\|")
                })
                .collect()
        })
        .collect();
    let mut table = Table {
        align: Vec::new(),
        rows,
    };
    table.normalize();
    let eol = eol(text);
    let (block, _) = table.render(eol);
    text.replace_range(start..end, "");
    let at = place(text, start, &block, eol);
    Some(caret(text, at + block.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURSOR: char = '‸';

    /// Runs `op` on `doc` (cursor marked with `‸`) and returns the result with the new
    /// cursor marked, or `None` if the op did not apply.
    fn run(doc: &str, op: Op) -> Option<String> {
        let pos = doc.chars().position(|c| c == CURSOR).unwrap();
        let mut text = doc.replace(CURSOR, "");
        let sel = apply(&mut text, pos..pos, op)?;
        text.insert(byte_at(&text, sel.start), CURSOR);
        Some(text)
    }

    const T: &str = "| a | bb |\n|---|:-:|\n| c | d |\n";

    fn with_cursor(text: &str, at: &str) -> String {
        text.replacen(at, &format!("{at}{CURSOR}"), 1)
    }

    #[test]
    fn next_and_prev_walk_cells_and_realign() {
        let doc = "a | bb\n-|:-:\nc‸ | d\n";
        assert_eq!(
            run(doc, Op::Next).unwrap(),
            "| a   | bb  |\n| --- | :-: |\n| c   | d‸   |\n"
        );
        // Last cell: Tab adds a row.
        assert_eq!(
            run("| a | b |\n|---|---|\n| c | d‸ |", Op::Next).unwrap(),
            "| a   | b   |\n| --- | --- |\n| c   | d   |\n| ‸    |     |"
        );
        assert_eq!(
            run("| a | b |\n|---|---|\n| ‸c | d |", Op::Prev).unwrap(),
            "| a   | b‸   |\n| --- | --- |\n| c   | d   |"
        );
        assert_eq!(
            run("| ‸a | b |\n|---|---|", Op::Prev).unwrap(),
            "| a‸   | b   |\n| --- | --- |"
        );
    }

    #[test]
    fn enter_moves_down_adds_rows_and_exits_on_empty_last_row() {
        let doc = "| a | b |\n|---|---|\n| c | d |\n";
        assert_eq!(
            run(&with_cursor(doc, "| a"), Op::NextRow).unwrap(),
            "| a   | b   |\n| --- | --- |\n| c‸   | d   |\n"
        );
        assert_eq!(
            run(&with_cursor(doc, "| d"), Op::NextRow).unwrap(),
            "| a   | b   |\n| --- | --- |\n| c   | d   |\n|     | ‸    |\n"
        );
        assert_eq!(
            run("| a | b |\n|---|---|\n|  |  ‸|\nnext", Op::NextRow).unwrap(),
            "| a   | b   |\n| --- | --- |\n‸\nnext"
        );
        assert_eq!(
            run("| a | b |\n|---|---|\n| ‸ | |", Op::NextRow).unwrap(),
            "| a   | b   |\n| --- | --- |\n‸"
        );
    }

    #[test]
    fn structure_ops() {
        let doc = "| a | b |\n|---|:-:|\n| c‸ | d |\n| e | f |";
        let ok = |op| run(doc, op).unwrap();
        assert_eq!(
            ok(Op::InsertRow { above: false }),
            "| a   | b   |\n| --- | :-: |\n| c   | d   |\n| ‸    |     |\n| e   | f   |"
        );
        assert_eq!(
            ok(Op::InsertRow { above: true }),
            "| a   | b   |\n| --- | :-: |\n| ‸    |     |\n| c   | d   |\n| e   | f   |"
        );
        assert_eq!(
            ok(Op::InsertCol { left: false }),
            "| a   |     | b   |\n| --- | --- | :-: |\n| c   | ‸    | d   |\n| e   |     | f   |"
        );
        assert_eq!(
            ok(Op::InsertCol { left: true }),
            "|     | a   | b   |\n| --- | --- | :-: |\n| ‸    | c   | d   |\n|     | e   | f   |"
        );
        assert_eq!(
            ok(Op::DeleteRow),
            "| a   | b   |\n| --- | :-: |\n| e‸   | f   |"
        );
        assert_eq!(ok(Op::DeleteCol), "| b   |\n| :-: |\n| d‸   |\n| f   |");
        assert_eq!(
            ok(Op::MoveRow { up: false }),
            "| a   | b   |\n| --- | :-: |\n| e   | f   |\n| c‸   | d   |"
        );
        assert_eq!(
            ok(Op::MoveCol { left: false }),
            "| b   | a   |\n| :-: | --- |\n| d   | c‸   |\n| f   | e   |"
        );
        assert_eq!(ok(Op::DeleteTable), "‸");
        // Edges change nothing.
        for op in [Op::MoveRow { up: true }, Op::MoveCol { left: true }] {
            assert_eq!(ok(op), doc);
        }
        let header = "| a‸ | b |\n|---|---|\n| c | d |";
        assert_eq!(run(header, Op::DeleteRow).unwrap(), header);
        assert_eq!(
            run(header, Op::InsertRow { above: true }).unwrap(),
            "| a   | b   |\n| --- | --- |\n| ‸    |     |\n| c   | d   |"
        );
        assert_eq!(
            run("x\n\n| a‸ |\n|---|\n| c |\n\ny", Op::DeleteCol).unwrap(),
            "x\n\n‸\ny"
        );
    }

    #[test]
    fn uneven_rows_are_filled_never_cut() {
        assert_eq!(
            run(
                "| a | b | New‸\n|---|---|\n| c |\n| d | e | f | g |",
                Op::Next
            )
            .unwrap(),
            concat!(
                "| a   | b   | New | ‸    |\n",
                "| --- | --- | --- | --- |\n",
                "| c   |     |     |     |\n",
                "| d   | e   | f   | g   |"
            )
        );
    }

    #[test]
    fn width_escapes_and_crlf() {
        assert_eq!(
            run(
                "| 名前 | é |\r\n|---|---|\r\n| a\\|b‸ | `x` |\r\n",
                Op::Prev
            )
            .unwrap(),
            "| 名前 | é‸   |\r\n| ---- | --- |\r\n| a\\|b | `x` |\r\n"
        );
    }

    #[test]
    fn not_a_table() {
        for doc in [
            "plain‸ text",
            "a | b‸\nno separator",
            "```\n| a | b‸ |\n|---|---|\n```\n",
            "    | a | b‸ |\n    |---|---|\n",
            "- | a | b‸ |\n  |---|---|\n",
            "> | a | b‸ |\n> |---|---|\n",
        ] {
            assert_eq!(run(doc, Op::Next), None, "{doc:?}");
        }
    }

    #[test]
    fn only_the_table_changes_and_no_cell_is_lost() {
        let before = "# Title\n\nsome | prose\n\n";
        let after = "\n\ntail | text\n";
        let table = "| x | `y\\|z` | 名 |\n|:--|--:|---|\n| 1 | 2 |\n| 3 | 4 | 5 |";
        let content = |t: &str| {
            let mut v: Vec<String> = t
                .lines()
                .filter(|l| separator(l).is_none())
                .flat_map(cells)
                .filter(|c| !c.is_empty())
                .collect();
            v.sort();
            v
        };
        let ops = [
            Op::Next,
            Op::Prev,
            Op::NextRow,
            Op::InsertRow { above: false },
            Op::InsertRow { above: true },
            Op::InsertCol { left: true },
            Op::InsertCol { left: false },
            Op::MoveRow { up: true },
            Op::MoveRow { up: false },
            Op::MoveCol { left: true },
            Op::MoveCol { left: false },
        ];
        for cursor in ["| x", "y\\|", "| 2", "| 5"] {
            for op in ops {
                let doc = format!("{before}{}{after}", with_cursor(table, cursor));
                let out = run(&doc, op).unwrap().replace(CURSOR, "");
                assert!(out.starts_with(before) && out.ends_with(after), "{op:?}");
                let new = &out[before.len()..out.len() - after.len()];
                assert_eq!(content(new), content(table), "{op:?} at {cursor}");
                // Formatting is stable: a second pass changes nothing but the cursor.
                let pos = new.chars().count() / 2;
                let mut again = new.to_owned();
                apply(&mut again, pos..pos, Op::Next).unwrap();
                let mut twice = again.clone();
                apply(&mut twice, 0..0, Op::Prev).unwrap();
                assert_eq!(again, twice);
            }
        }
    }

    #[test]
    fn untouched_tables_stay_byte_for_byte() {
        let doc = format!("{T}\n| x |y|\n|-|-|\n| ‸1 |2|\n");
        let out = run(&doc, Op::Next).unwrap();
        assert!(out.starts_with(T));
    }

    #[test]
    fn insert_on_own_lines() {
        let ins = |doc: &str, cols, rows| {
            let pos = doc.chars().position(|c| c == CURSOR).unwrap();
            let mut text = doc.replace(CURSOR, "");
            let sel = insert(&mut text, pos..pos, cols, rows);
            text.insert(byte_at(&text, sel.start), CURSOR);
            text
        };
        assert_eq!(
            ins("‸", 2, 1),
            "| ‸    |     |\n| --- | --- |\n|     |     |\n"
        );
        assert_eq!(
            ins("intro‸ text\nnext", 1, 1),
            "intro text\n\n| ‸    |\n| --- |\n|     |\n\nnext"
        );
        assert_eq!(
            ins("intro\n\n‸\n\nnext", 1, 0),
            "intro\n\n| ‸    |\n| --- |\n\nnext"
        );
    }

    #[test]
    fn spreadsheet_clipboard_becomes_a_table() {
        let paste_at = |doc: &str, clip: &str| {
            let pos = doc.chars().position(|c| c == CURSOR).unwrap();
            let mut text = doc.replace(CURSOR, "");
            let sel = paste(&mut text, pos..pos, clip)?;
            text.insert(byte_at(&text, sel.start), CURSOR);
            Some(text)
        };
        assert_eq!(
            paste_at("‸", "Name\tNote\r\nAnn\t\"two\nlines\"\r\nBob\ta|b\r\n").unwrap(),
            concat!(
                "| Name | Note         |\n",
                "| ---- | ------------ |\n",
                "| Ann  | two<br>lines |\n",
                "| Bob  | a\\|b         |‸\n"
            )
        );
        assert_eq!(
            paste_at("text‸", "a\tb\n\"say \"\"hi\"\"\"\tc").unwrap(),
            "text\n\n| a        | b   |\n| -------- | --- |\n| say \"hi\" | c   |‸\n"
        );
        for clip in ["one\trow", "a\tb\nc", "plain prose\nmore", "a\nb"] {
            assert_eq!(paste_at("‸", clip), None, "{clip:?}");
        }
        assert_eq!(paste_at("| x‸ |\n|---|\n", "a\tb\nc\td"), None);
        assert_eq!(paste_at("```\n‸\n```", "a\tb\nc\td"), None);
    }

    #[test]
    #[ignore = "timing: cargo test --release -- --ignored"]
    fn table_ops_on_large_doc_are_fast() {
        let filler = "Some paragraph text with *emphasis* and `code`.\n\n".repeat(20_000);
        let mut table = String::from("| a | b | c |\n|---|---|---|\n");
        table.push_str(&"| 1 | 2 | 3 |\n".repeat(200));
        let doc = format!("{filler}{table}{filler}");
        let pos = filler.chars().count() + 2;
        for op in [
            Op::Next,
            Op::NextRow,
            Op::InsertCol { left: false },
            Op::DeleteRow,
        ] {
            let mut text = doc.clone();
            let t = std::time::Instant::now();
            apply(&mut text, pos..pos, op).unwrap();
            assert!(t.elapsed().as_millis() < 50, "{op:?}: {:?}", t.elapsed());
        }
    }
}
