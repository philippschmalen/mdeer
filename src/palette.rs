//! Commands and the `Ctrl+P` command palette (ADR-0007).

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use egui::text::{CCursor, CCursorRange};

use crate::table::Op;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    Palette,
    Open,
    CloseTab,
    PrevTab,
    NextTab,
    ToggleView,
    Bold,
    Italic,
    Link,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    InsertTable,
    Table(Op),
}

/// Where a command can run.
#[derive(PartialEq)]
pub enum Scope {
    Always,
    Source,
    /// Source view with the cursor in a table.
    Table,
}

const CTRL: Modifiers = Modifiers::COMMAND;
const ALT_SHIFT: Modifiers = Modifiers::ALT.plus(Modifiers::SHIFT);
const CTRL_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

impl Command {
    /// Dispatch order: egui matches shortcuts ignoring extra Shift/Alt, so longer chords
    /// come before the shorter ones they contain (checked by a test).
    pub const ALL: &[Command] = &[
        Command::Palette,
        Command::Open,
        Command::CloseTab,
        Command::PrevTab,
        Command::NextTab,
        Command::ToggleView,
        Command::Bold,
        Command::Italic,
        Command::Link,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ZoomReset,
        Command::InsertTable,
        Command::Table(Op::InsertRow { above: false }),
        Command::Table(Op::InsertRow { above: true }),
        Command::Table(Op::InsertCol { left: false }),
        Command::Table(Op::InsertCol { left: true }),
        Command::Table(Op::MoveRow { up: true }),
        Command::Table(Op::MoveRow { up: false }),
        Command::Table(Op::MoveCol { left: true }),
        Command::Table(Op::MoveCol { left: false }),
        Command::Table(Op::DeleteRow),
        Command::Table(Op::DeleteCol),
        Command::Table(Op::DeleteTable),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Command::Palette => "Command palette",
            Command::Open => "Open file",
            Command::CloseTab => "Close tab",
            Command::PrevTab => "Previous tab",
            Command::NextTab => "Next tab",
            Command::ToggleView => "Toggle reading / source view",
            Command::Bold => "Bold",
            Command::Italic => "Italic",
            Command::Link => "Link",
            Command::ZoomIn => "Zoom in",
            Command::ZoomOut => "Zoom out",
            Command::ZoomReset => "Reset zoom",
            Command::InsertTable => "Insert table…",
            Command::Table(op) => match op {
                Op::InsertRow { above: false } => "Insert row below",
                Op::InsertRow { above: true } => "Insert row above",
                Op::InsertCol { left: false } => "Insert column right",
                Op::InsertCol { left: true } => "Insert column left",
                Op::MoveRow { up: true } => "Move row up",
                Op::MoveRow { up: false } => "Move row down",
                Op::MoveCol { left: true } => "Move column left",
                Op::MoveCol { left: false } => "Move column right",
                Op::DeleteRow => "Delete row",
                Op::DeleteCol => "Delete column",
                Op::DeleteTable => "Delete table",
                Op::Next | Op::Prev | Op::NextRow => "",
            },
        }
    }

    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        let (modifiers, key) = match self {
            Command::Palette => (CTRL, Key::P),
            Command::Open => (CTRL, Key::O),
            Command::CloseTab => (CTRL, Key::W),
            Command::PrevTab => (CTRL_SHIFT, Key::Tab),
            Command::NextTab => (CTRL, Key::Tab),
            Command::ToggleView => (CTRL, Key::E),
            Command::Bold => (CTRL, Key::B),
            Command::Italic => (CTRL, Key::I),
            Command::Link => (CTRL, Key::K),
            Command::ZoomIn => (CTRL, Key::Equals),
            Command::ZoomOut => (CTRL, Key::Minus),
            Command::ZoomReset => (CTRL, Key::Num0),
            Command::InsertTable => return None,
            Command::Table(op) => match op {
                Op::InsertRow { above } => {
                    (ALT_SHIFT, if above { Key::ArrowUp } else { Key::ArrowDown })
                }
                Op::InsertCol { left } => (
                    ALT_SHIFT,
                    if left {
                        Key::ArrowLeft
                    } else {
                        Key::ArrowRight
                    },
                ),
                Op::MoveRow { up } => (
                    Modifiers::ALT,
                    if up { Key::ArrowUp } else { Key::ArrowDown },
                ),
                Op::MoveCol { left } => (
                    Modifiers::ALT,
                    if left {
                        Key::ArrowLeft
                    } else {
                        Key::ArrowRight
                    },
                ),
                Op::DeleteRow => (CTRL_SHIFT, Key::Backspace),
                Op::DeleteCol => (CTRL_SHIFT, Key::Delete),
                Op::DeleteTable | Op::Next | Op::Prev | Op::NextRow => return None,
            },
        };
        Some(KeyboardShortcut::new(modifiers, key))
    }

    pub fn scope(self) -> Scope {
        match self {
            Command::Bold | Command::Italic | Command::Link | Command::InsertTable => Scope::Source,
            Command::Table(_) => Scope::Table,
            _ => Scope::Always,
        }
    }

    /// egui handles the zoom keys itself; the palette only lists them.
    pub fn handled_by_egui(self) -> bool {
        matches!(
            self,
            Command::ZoomIn | Command::ZoomOut | Command::ZoomReset
        )
    }
}

/// Case-insensitive: the query's characters appear in `label` in order.
pub fn matches(query: &str, label: &str) -> bool {
    let mut label = label.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .all(|q| label.any(|l| l == q))
}

/// `CxR` (or `C×R`): columns 1–50, body rows 1–500.
pub fn parse_size(s: &str) -> Option<(usize, usize)> {
    let (c, r) = s.split_once(['x', 'X', '×'])?;
    let (c, r): (usize, usize) = (c.trim().parse().ok()?, r.trim().parse().ok()?);
    ((1..=50).contains(&c) && (1..=500).contains(&r)).then_some((c, r))
}

pub enum Outcome {
    Open,
    Close,
    Run(Command),
    InsertTable(usize, usize),
}

pub struct Palette {
    available: Vec<Command>,
    query: String,
    selected: usize,
    /// Asking for the size of a new table.
    sizing: bool,
    invalid: bool,
}

impl Palette {
    /// `available` are the commands that can run right now, in display order.
    pub fn new(available: Vec<Command>) -> Self {
        Self {
            available: available
                .into_iter()
                .filter(|&c| c != Command::Palette)
                .collect(),
            query: String::new(),
            selected: 0,
            sizing: false,
            invalid: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let mut outcome = Outcome::Open;
        egui::Modal::new(egui::Id::unique("command-palette")).show(ctx, |ui| {
            ui.set_width(440.0);
            let (esc, up, down, enter) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::Escape),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::Enter),
                )
            });
            if esc {
                outcome = Outcome::Close;
                return;
            }
            let hint = if self.sizing {
                "Table size: columns x rows"
            } else {
                "Type a command"
            };
            let edit = egui::TextEdit::singleline(&mut self.query)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .show(ui);
            edit.response.request_focus();
            let edit_id = edit.response.id;
            if self.sizing {
                ui.weak(if self.invalid {
                    "Enter a size like 4x3 (up to 50 columns, 500 rows)"
                } else {
                    "Enter inserts the table, Esc cancels"
                });
                if enter {
                    match parse_size(&self.query) {
                        Some((c, r)) => outcome = Outcome::InsertTable(c, r),
                        None => self.invalid = true,
                    }
                }
                return;
            }
            let list: Vec<Command> = self
                .available
                .iter()
                .copied()
                .filter(|c| matches(&self.query, c.label()))
                .collect();
            if down {
                self.selected += 1;
            }
            if up {
                self.selected = self.selected.saturating_sub(1);
            }
            self.selected = self.selected.min(list.len().saturating_sub(1));
            let mut chosen = enter.then(|| list.get(self.selected).copied()).flatten();
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for (i, &cmd) in list.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let row = ui.selectable_label(i == self.selected, cmd.label());
                            if i == self.selected && (up || down) {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                chosen = Some(cmd);
                            }
                            if let Some(sc) = cmd.shortcut() {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.weak(ui.ctx().format_shortcut(&sc));
                                    },
                                );
                            }
                        });
                    }
                    if list.is_empty() {
                        ui.weak("No matching command");
                    }
                });
            match chosen {
                Some(Command::InsertTable) => {
                    self.sizing = true;
                    self.query = "3x2".to_owned();
                    // Selected, so typing a size replaces it.
                    let mut state = edit.state;
                    let all = CCursorRange::two(CCursor::new(0), CCursor::new(3));
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), edit_id);
                }
                Some(cmd) => outcome = Outcome::Run(cmd),
                None => {}
            }
        });
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_chords_come_first() {
        let shortcuts: Vec<_> = Command::ALL.iter().filter_map(|c| c.shortcut()).collect();
        for (i, later) in shortcuts.iter().enumerate() {
            for earlier in &shortcuts[..i] {
                assert!(
                    earlier.logical_key != later.logical_key
                        || !later.modifiers.matches_logically(earlier.modifiers),
                    "{earlier:?} shadows {later:?}"
                );
            }
        }
    }

    #[test]
    fn every_listed_command_has_a_label() {
        for c in Command::ALL {
            assert!(!c.label().is_empty(), "{c:?}");
        }
    }

    #[test]
    fn fuzzy_matching() {
        assert!(matches("irow", "Insert row below"));
        assert!(matches("ZOOM in", "Zoom in"));
        assert!(matches("", "Bold"));
        assert!(!matches("wor", "Insert row below"));
        assert!(!matches("bolder", "Bold"));
    }

    #[test]
    fn table_sizes() {
        assert_eq!(parse_size("4x3"), Some((4, 3)));
        assert_eq!(parse_size(" 2 × 10 "), Some((2, 10)));
        assert_eq!(parse_size("3X1"), Some((3, 1)));
        for bad in ["", "3", "0x2", "51x1", "2x0", "2x501", "ax2", "2x3x4"] {
            assert_eq!(parse_size(bad), None, "{bad}");
        }
    }
}
