//! Modal and floating windows: preferences, upload, about, unsaved changes
//! and messages.

pub mod preferences;
pub mod upload;

use std::path::PathBuf;

use egui::{Align2, RichText};
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::document::DiskVersion;

/// Something that would discard unsaved changes, waiting for confirmation.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    New,
    Open,
    OpenPath(PathBuf),
    OpenExample(usize),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalChangeChoice {
    Reload,
    KeepLocal,
    SaveCopy,
}

/// An on-disk edit that conflicts with unsaved editor contents.
pub struct ExternalChange {
    pub path: PathBuf,
    pub disk: DiskVersion,
    pub show_comparison: bool,
    pub rerun_after_resolution: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

pub struct Message {
    pub title: String,
    pub text: String,
    pub kind: MessageKind,
}

#[derive(Default)]
pub struct Dialogs {
    pub preferences: bool,
    pub preferences_page: preferences::Page,
    pub about: bool,
    pub upload: Option<upload::UploadDialog>,
    pub unsaved: Option<Intent>,
    pub external_change: Option<ExternalChange>,
    pub message: Option<Message>,
}

impl Dialogs {
    pub fn info(&mut self, title: &str, text: impl Into<String>) {
        self.message = Some(Message {
            title: title.to_owned(),
            text: text.into(),
            kind: MessageKind::Info,
        });
    }

    pub fn error(&mut self, title: &str, text: impl Into<String>) {
        self.message = Some(Message {
            title: title.to_owned(),
            text: text.into(),
            kind: MessageKind::Error,
        });
    }
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let mut actions = Vec::new();

    if state.dialogs.preferences {
        let dialogs = &mut state.dialogs;
        preferences::window(
            ctx,
            &mut dialogs.preferences,
            &mut dialogs.preferences_page,
            &mut state.settings,
            &mut actions,
        );
    }

    if let Some(upload) = &mut state.dialogs.upload
        && !upload.window(ctx, &mut actions)
    {
        state.dialogs.upload = None;
    }

    if state.dialogs.about {
        about(ctx, state);
    }

    if let Some(intent) = state.dialogs.unsaved.clone()
        && let Some(choice) = unsaved_changes(ctx, &state.document.display_name())
    {
        state.dialogs.unsaved = None;
        state.resolve_unsaved(intent, choice);
    }

    let document_name = state.document.display_name();
    let external_choice = state
        .dialogs
        .external_change
        .as_mut()
        .and_then(|change| external_change(ctx, &document_name, &state.document.text, change));
    if let Some(choice) = external_choice
        && let Some(change) = state.dialogs.external_change.take()
    {
        state.resolve_external_change(change, choice);
    }

    if let Some(message) = &state.dialogs.message
        && message_box(ctx, message)
    {
        state.dialogs.message = None;
    }

    state.queue(actions);
}

fn modal_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(18))
}

fn about(ctx: &egui::Context, state: &mut AppState) {
    let response = egui::Modal::new(egui::Id::new("about"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.vertical_centered(|ui| {
                if let Some(logo) = state.logo(ctx) {
                    ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::Vec2::splat(72.0)));
                }
                ui.add_space(6.0);
                ui.label(RichText::new("CQ-editor").size(22.0).strong());
                ui.weak(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(8.0);
                ui.label("CadQuery script editor and 3D viewer.");
                ui.weak("Rust interface (egui + wgpu) with a Python worker running CadQuery.");
                ui.add_space(8.0);
                if let Some(info) = state.worker.info() {
                    ui.weak(format!(
                        "Python {} · CadQuery {}",
                        info.python, info.cadquery
                    ));
                }
                ui.hyperlink_to(
                    format!("{} Source code", icon::GITHUB_LOGO),
                    "https://github.com/CadQuery/CQ-editor",
                );
                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    ui.close();
                }
            });
        });
    if response.should_close() {
        state.dialogs.about = false;
    }
}

/// Returns the user's choice once made.
fn unsaved_changes(ctx: &egui::Context, name: &str) -> Option<UnsavedChoice> {
    let mut choice = None;
    let response = egui::Modal::new(egui::Id::new("unsaved_changes"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::WARNING)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("Unsaved changes").strong().size(16.0));
                    ui.label(format!("Save the changes to {name} first?"));
                });
            });
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    choice = Some(UnsavedChoice::Cancel);
                }
                if ui.button("Don't save").clicked() {
                    choice = Some(UnsavedChoice::Discard);
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Save").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(UnsavedChoice::Save);
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(UnsavedChoice::Cancel);
    }
    choice
}

/// Returns a resolution only after an explicit choice. Comparing keeps the
/// dialog open and Escape takes the lossless "keep local" path.
fn external_change(
    ctx: &egui::Context,
    name: &str,
    local_text: &str,
    change: &mut ExternalChange,
) -> Option<ExternalChangeChoice> {
    let mut choice = None;
    let response = egui::Modal::new(egui::Id::new("external_change"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(if change.show_comparison { 760.0 } else { 500.0 });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::WARNING)
                        .size(28.0)
                        .color(ui.visuals().warn_fg_color),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new("File changed on disk").strong().size(16.0));
                    ui.label(format!(
                        "{name} was changed by another program while you have unsaved edits."
                    ));
                    ui.weak("Your editor contents have not been replaced.");
                });
            });

            if change.show_comparison {
                ui.add_space(10.0);
                ui.columns(2, |columns| {
                    let show_version =
                        |column: &mut egui::Ui, title: &str, text: &str, id: &str| {
                            column.label(RichText::new(title).strong());
                            egui::Frame::group(column.style()).show(column, |ui| {
                                egui::ScrollArea::both().id_salt(id).max_height(300.0).show(
                                    ui,
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(text).monospace())
                                                .selectable(true),
                                        );
                                    },
                                );
                            });
                        };
                    show_version(&mut columns[0], "Your version", local_text, "local_change");
                    show_version(
                        &mut columns[1],
                        "Version on disk",
                        &change.disk.text,
                        "disk_change",
                    );
                });
            }

            ui.add_space(12.0);
            ui.weak(
                "Reload creates a recovery backup before replacing the editor contents. \
                 Saving a copy preserves your version separately, then reloads the disk version.",
            );
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Keep my version").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .clicked()
                {
                    choice = Some(ExternalChangeChoice::KeepLocal);
                }
                if ui.button("Reload from disk").clicked() {
                    choice = Some(ExternalChangeChoice::Reload);
                }
                if ui.button("Save a copy…").clicked() {
                    choice = Some(ExternalChangeChoice::SaveCopy);
                }
                let compare = if change.show_comparison {
                    "Hide comparison"
                } else {
                    "Compare changes"
                };
                if ui.button(compare).clicked() {
                    change.show_comparison = !change.show_comparison;
                }
            });
        });
    if response.should_close() && choice.is_none() {
        choice = Some(ExternalChangeChoice::KeepLocal);
    }
    choice
}

/// Returns `true` once dismissed.
fn message_box(ctx: &egui::Context, message: &Message) -> bool {
    let mut dismissed = false;
    let response = egui::Modal::new(egui::Id::new("message_box"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            let (glyph, color) = match message.kind {
                MessageKind::Info => (icon::INFO, ui.visuals().hyperlink_color),
                MessageKind::Error => (icon::X_CIRCLE, ui.visuals().error_fg_color),
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(glyph).size(26.0).color(color));
                ui.label(RichText::new(&message.title).strong().size(16.0));
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&message.text).selectable(true));
                });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    dismissed = true;
                }
            });
        });
    dismissed || response.should_close()
}

/// Floating hint shown while the worker could not start, with ways out.
pub fn python_problem(ctx: &egui::Context, reason: &str, python: &str, actions: &mut Vec<Action>) {
    egui::Area::new(egui::Id::new("python_problem"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -40.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::WARNING_OCTAGON).size(22.0).color(ui.visuals().error_fg_color));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("CadQuery is not available").strong());
                        ui.label(reason);
                        ui.weak(format!("Interpreter: {python}"));
                        ui.weak("Install CadQuery in that environment (pip install cadquery) or choose another interpreter.");
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button(format!("{} Preferences…", icon::GEAR_SIX)).clicked() {
                        actions.push(Action::Preferences);
                    }
                    if ui.button(format!("{} Retry", icon::ARROW_CLOCKWISE)).clicked() {
                        actions.push(Action::RestartPython);
                    }
                });
            });
        });
}
