//! The object tree: shown CadQuery objects, the axis helpers and the
//! properties of the current object.

use egui::{Color32, RichText, Sense, Vec2};
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::scene::{Rgb, SceneNode};
use crate::worker::protocol::ExportFormat;

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.scene.is_empty(),
                        egui::Button::new(format!("{} Clear all", icon::TRASH)),
                    )
                    .on_hover_text("Remove all objects")
                    .clicked()
                {
                    actions.push(Action::ClearObjects);
                }
                if ui
                    .add_enabled(
                        !state.scene.selection.is_empty(),
                        egui::Button::new(icon::X),
                    )
                    .on_hover_text("Remove the selected objects")
                    .clicked()
                {
                    actions.push(Action::RemoveSelected);
                }
                ui.add_enabled_ui(!state.scene.is_empty(), |ui| {
                    ui.menu_button(format!("{} Export", icon::EXPORT), |ui| {
                        export_menu(ui, &mut actions);
                    })
                    .response
                    .on_hover_text("Export the selection, or everything when nothing is selected");
                });
            });
        });
    ui.separator();

    if !state.scene.selection.is_empty() {
        egui::Panel::bottom("object_properties")
            .resizable(true)
            .default_size(130.0)
            .min_size(70.0)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)))
            .show(ui, |ui| properties(ui, state, &mut actions));
    }

    // The tree takes what the properties panel leaves (panels must be
    // followed by a central panel to keep out of their space).
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            tree(ui, state, &mut actions);
        });

    state.queue(actions);
}

fn tree(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let default = state.default_color();
            egui::CollapsingHeader::new(RichText::new("CQ models").strong())
                .default_open(true)
                .show(ui, |ui| {
                    if state.scene.is_empty() {
                        ui.weak("Nothing shown yet — press F5 to render");
                    }
                    for node in &state.scene.nodes {
                        node_row(ui, state, node, default, actions);
                    }
                });
            egui::CollapsingHeader::new(RichText::new("Helpers").strong())
                .default_open(true)
                .show(ui, |ui| {
                    for axis in &mut state.scene.axes {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut axis.visible, "");
                            swatch(ui, axis.color, 1.0);
                            ui.label(format!("{} axis", axis.name));
                        });
                    }
                });
        });
}

fn export_menu(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    for format in ExportFormat::ALL {
        if ui
            .button(format!("Export as {}…", format.label()))
            .clicked()
        {
            actions.push(Action::Export(format));
        }
    }
}

fn node_row(
    ui: &mut egui::Ui,
    state: &AppState,
    node: &SceneNode,
    default: Rgb,
    actions: &mut Vec<Action>,
) {
    let id = ui.make_persistent_id(("scene_node", &node.key));
    let mut header = |ui: &mut egui::Ui| {
        let mut visible = node.visible;
        if ui
            .checkbox(&mut visible, "")
            .on_hover_text("Show / hide")
            .changed()
        {
            actions.push(Action::SetVisible {
                key: node.key.clone(),
                visible,
            });
        }
        if let Some((color, alpha)) = node.display_color(default) {
            swatch(ui, color, alpha);
        }
        let selected = state.scene.is_selected(&node.key);
        let mut text = RichText::new(&node.name);
        if !node.visible {
            text = text.weak();
        }
        let response = ui
            .selectable_label(selected, text)
            .on_hover_text(&node.kind);
        if response.clicked() {
            actions.push(Action::Select {
                key: Some(node.key.clone()),
                extend: ui.input(|i| i.modifiers.command),
            });
        }
        if response.double_clicked() {
            actions.push(Action::FitSelection);
        }
        response.context_menu(|ui| {
            if !selected {
                actions.push(Action::Select {
                    key: Some(node.key.clone()),
                    extend: false,
                });
            }
            if ui
                .button(format!("{} Zoom to", icon::MAGNIFYING_GLASS_PLUS))
                .clicked()
            {
                actions.push(Action::FitSelection);
                ui.close();
            }
            let (label, visible) = if node.visible {
                ("Hide", false)
            } else {
                ("Show", true)
            };
            if ui
                .button(format!(
                    "{} {label}",
                    if visible { icon::EYE } else { icon::EYE_SLASH }
                ))
                .clicked()
            {
                actions.push(Action::SetVisible {
                    key: node.key.clone(),
                    visible,
                });
                ui.close();
            }
            ui.separator();
            export_menu(ui, actions);
            ui.separator();
            if ui.button(format!("{} Remove", icon::TRASH)).clicked() {
                actions.push(Action::RemoveSelected);
                ui.close();
            }
        });
    };

    if node.children.is_empty() {
        ui.horizontal(|ui| {
            // Line up with rows that have an expand arrow.
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
            header(ui);
        });
    } else {
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, header)
            .body(|ui| {
                for child in &node.children {
                    node_row(ui, state, child, default, actions);
                }
            });
    }
}

/// A small rounded square showing a color.
fn swatch(ui: &mut egui::Ui, color: Rgb, alpha: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    let [r, g, b] = color.map(|c| (c * 255.0).round() as u8);
    let fill = Color32::from_rgba_unmultiplied(r, g, b, (alpha.max(0.25) * 255.0) as u8);
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().rect_stroke(
        rect,
        3.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

fn properties(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let default = state.default_color();
    let Some(key) = state.scene.selection.last().cloned() else {
        return;
    };
    let Some(node) = state.scene.find_mut(&key) else {
        return;
    };

    ui.strong(&node.name);
    egui::Grid::new("object_properties_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Type");
            ui.weak(&node.kind);
            ui.end_row();

            ui.label("Visible");
            let mut visible = node.visible;
            if ui.checkbox(&mut visible, "").changed() {
                actions.push(Action::SetVisible {
                    key: key.clone(),
                    visible,
                });
            }
            ui.end_row();

            if !node.parts.is_empty() {
                let (color, alpha) = node
                    .parts
                    .first()
                    .map(|part| node.part_color(part, default))
                    .unwrap_or((default, 1.0));
                let mut srgb = color.map(|c| (c * 255.0).round() as u8);
                ui.label("Color");
                ui.horizontal(|ui| {
                    if ui.color_edit_button_srgb(&mut srgb).changed() {
                        node.overrides.color = Some(srgb.map(|c| f32::from(c) / 255.0));
                    }
                    if node.overrides != Default::default()
                        && ui
                            .small_button("Reset")
                            .on_hover_text("Use the script's colors")
                            .clicked()
                    {
                        node.overrides = Default::default();
                    }
                });
                ui.end_row();

                ui.label("Opacity");
                let mut opacity = alpha;
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.05..=1.0).fixed_decimals(2))
                    .changed()
                {
                    node.overrides.alpha = Some(opacity);
                }
                ui.end_row();
            }

            let bounds = node.subtree_bounds();
            if !bounds.is_empty() {
                let size = bounds.size();
                ui.label("Size");
                ui.weak(format!(
                    "{} × {} × {}",
                    number(size.x),
                    number(size.y),
                    number(size.z)
                ));
                ui.end_row();
            }
            let triangles = node.triangle_count();
            if triangles > 0 {
                ui.label("Triangles");
                ui.weak(triangles.to_string());
                ui.end_row();
            }
        });
    if node.inspectable {
        ui.add_space(4.0);
        ui.weak(format!(
            "{} Its construction stack is in CQ Inspector",
            icon::STACK
        ));
    }
}

/// Compact number formatting for sizes.
fn number(value: f32) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
