//! Tokyo Night (Night variant) palette applied to egui's dark visuals.

use eframe::egui::{Color32, Context, Stroke, Visuals};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const BG: Color32 = hex(0x1a1b26);
pub const BG_DARK: Color32 = hex(0x16161e);
pub const BG_HIGHLIGHT: Color32 = hex(0x292e42);
pub const BG_VISUAL: Color32 = hex(0x283457);
pub const FG: Color32 = hex(0xc0caf5);
pub const FG_DARK: Color32 = hex(0xa9b1d6);
pub const TERMINAL_BLACK: Color32 = hex(0x414868);
pub const BLUE: Color32 = hex(0x7aa2f7);
pub const CYAN: Color32 = hex(0x7dcfff);
pub const YELLOW: Color32 = hex(0xe0af68);
pub const RED: Color32 = hex(0xf7768e);

pub fn apply(ctx: &Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.faint_bg_color = BG_DARK;
    v.extreme_bg_color = BG_DARK;
    v.code_bg_color = BG_HIGHLIGHT;
    v.hyperlink_color = CYAN;
    v.warn_fg_color = YELLOW;
    v.error_fg_color = RED;
    v.selection.bg_fill = BG_VISUAL;
    v.selection.stroke = Stroke::new(1.0, FG);
    v.window_stroke = Stroke::new(1.0, TERMINAL_BLACK);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG;
    w.noninteractive.bg_stroke = Stroke::new(1.0, TERMINAL_BLACK);
    w.noninteractive.fg_stroke = Stroke::new(1.0, FG_DARK);
    w.inactive.bg_fill = BG_HIGHLIGHT;
    w.inactive.weak_bg_fill = BG_HIGHLIGHT;
    w.inactive.fg_stroke = Stroke::new(1.0, FG_DARK);
    w.hovered.bg_fill = TERMINAL_BLACK;
    w.hovered.weak_bg_fill = TERMINAL_BLACK;
    w.hovered.fg_stroke = Stroke::new(1.5, FG);
    w.active.bg_fill = BG_VISUAL;
    w.active.weak_bg_fill = BG_VISUAL;
    // egui derives "strong" text (headings, bold) from the active stroke;
    // default fonts have no bold face, so colour carries the emphasis.
    w.active.fg_stroke = Stroke::new(2.0, BLUE);

    ctx.set_visuals(v);
}
