//! The CadQuery object inspector: the construction stack of the selected
//! workplane, newest step first. Selecting a step shows its objects and its
//! workplane grid in the 3D view (when "Show in view" is on).

use egui::RichText;
use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::commands::Action;
use crate::scene::NodeKey;
use crate::worker::protocol::StackFrame;

#[derive(Default)]
pub struct InspectorState {
    /// Overlay the selected step in the 3D view.
    pub show_in_view: bool,
    /// The inspected object and its name.
    pub key: Option<NodeKey>,
    pub name: String,
    pub frames: Vec<StackFrame>,
    /// Selected (frame, item); `None` item selects the whole frame.
    pub selected: Option<(usize, Option<usize>)>,
    pub loading: bool,
}

impl InspectorState {
    pub fn clear(&mut self) {
        self.key = None;
        self.name.clear();
        self.frames.clear();
        self.selected = None;
        self.loading = false;
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut actions = Vec::new();
    let inspector = &mut state.inspector;

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        inspector.show_in_view,
                        format!("{} Show in view", icon::EYE),
                    )
                    .on_hover_text("Overlay the selected step on the model, with its workplane")
                    .clicked()
                {
                    actions.push(Action::ToggleInspect);
                }
                if !inspector.name.is_empty() {
                    ui.weak(&inspector.name);
                }
            });
        });
    ui.separator();

    if inspector.key.is_none() {
        super::empty_state(
            ui,
            icon::STACK,
            "Select a workplane in Objects to inspect it",
        );
        state.queue(actions);
        return;
    }
    if inspector.loading {
        ui.centered_and_justified(|ui| ui.spinner());
        state.queue(actions);
        return;
    }

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (frame_index, frame) in inspector.frames.iter().enumerate() {
                let id = ui.make_persistent_id(("inspector_frame", frame_index));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    frame_index == 0,
                )
                .show_header(ui, |ui| {
                    let selected = inspector.selected == Some((frame_index, None));
                    let text = RichText::new(format!("{}  {}", frame.type_name, frame.value));
                    if ui
                        .add(egui::Button::selectable(selected, text).truncate())
                        .clicked()
                    {
                        actions.push(Action::InspectEntry {
                            frame: frame_index,
                            item: None,
                        });
                    }
                    ui.weak(format!("{} objects", frame.items.len()));
                })
                .body(|ui| {
                    for (item_index, item) in frame.items.iter().enumerate() {
                        let selected = inspector.selected == Some((frame_index, Some(item_index)));
                        let text = RichText::new(format!("{}  {}", item.type_name, item.value));
                        let label = if item.displayable { text } else { text.weak() };
                        if ui
                            .add(egui::Button::selectable(selected, label).truncate())
                            .clicked()
                        {
                            actions.push(Action::InspectEntry {
                                frame: frame_index,
                                item: Some(item_index),
                            });
                        }
                    }
                });
            }
        });
    state.queue(actions);
}
