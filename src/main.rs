//! CQ-editor: a CadQuery script editor and 3D viewer.
//!
//! The interface is written in Rust (egui + wgpu); CadQuery itself runs in a
//! Python worker process (`worker/cq_worker`).

// No console window for release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod dialogs;
mod document;
mod editor;
mod embedded;
mod file_watcher;
mod logging;
mod panels;
mod scene;
mod settings;
mod spatial;
mod theme;
mod ui;
mod viewer;
mod worker;

use std::path::PathBuf;

use app::{CqEditorApp, StartupOptions};

const USAGE: &str = "Usage: cq-editor [--python PATH] [FILE]

  FILE            CadQuery script to open
  --python PATH   Python interpreter with CadQuery (overrides the preferences)";

fn main() -> eframe::Result {
    logging::init();

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CQ-editor")
            .with_app_id("cq-editor")
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(window_icon()),
        persist_window: true,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "cq-editor",
        native_options,
        Box::new(move |cc| Ok(Box::new(CqEditorApp::new(cc, options)))),
    )
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<StartupOptions, String> {
    let mut options = StartupOptions::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--python" => {
                options.python = Some(args.next().ok_or("--python needs a path")?);
            }
            flag if flag.starts_with('-') => return Err(format!("Unknown option {flag}")),
            file if options.file.is_none() => options.file = Some(PathBuf::from(file)),
            extra => return Err(format!("Unexpected argument {extra}")),
        }
    }
    Ok(options)
}

fn window_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icons/cadquery_logo_dark.ico");
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Ico) {
        Ok(image) => {
            let image = image
                .resize(128, 128, image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            egui::IconData {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            }
        }
        Err(err) => {
            log::warn!("Cannot decode the application icon: {err}");
            egui::IconData::default()
        }
    }
}

#[cfg(test)]
#[path = "../tests/rust/app/test_cli.rs"]
mod tests;
