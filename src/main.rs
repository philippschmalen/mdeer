#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod doc;
mod source;
mod theme;

use std::path::Path;
use std::process::ExitCode;

use doc::Doc;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

struct App {
    doc: Doc,
}

const CTRL: Modifiers = Modifiers::COMMAND;

fn pressed(ui: &egui::Ui, key: Key) -> bool {
    ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(CTRL, key)))
}

impl App {
    /// Conflict or save error, with the actions that resolve it.
    fn banner(&mut self, ui: &mut egui::Ui) {
        let doc = &mut self.doc;
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
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let focus_change = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::WindowFocused(f) => Some(*f),
                _ => None,
            })
        });
        match focus_change {
            Some(true) => self.doc.sync_from_disk(),
            Some(false) => self.doc.autosave(true),
            None => self.doc.autosave(false),
        }
        if self.doc.edited_at.is_some() {
            ui.ctx().request_repaint_after(doc::AUTOSAVE_DELAY);
        }

        let toggled = pressed(ui, Key::E);
        if toggled {
            self.doc.toggle_editing();
        }
        self.banner(ui);
        let doc = &mut self.doc;
        egui::CentralPanel::default().show(ui, |ui| {
            if doc.editing {
                doc.source_view(ui, toggled);
            } else {
                doc.reading_view(ui);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.doc.autosave(true);
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
