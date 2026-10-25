#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod theme;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};

struct Doc {
    path: PathBuf,
    text: String,
    cache: CommonMarkCache,
    /// `file://<dir>/` so relative image paths resolve next to the file.
    image_base: String,
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let doc = &mut self.doc;
            CommonMarkViewer::new()
                .default_implicit_uri_scheme(&doc.image_base)
                .enable_scroll_to_heading(true)
                .show_scrollable(
                    egui::Id::unique(&doc.path),
                    ui,
                    &mut doc.cache,
                    &CommonMarkScrollOptions::default().viewport_cache(true),
                    &doc.text,
                );
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
