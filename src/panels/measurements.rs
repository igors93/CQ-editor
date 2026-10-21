use egui_phosphor::regular as icon;

use crate::app::AppState;
use crate::worker::protocol::{EntityMeasurement, Measurement, PairMeasurement, TopologyKind};

#[derive(Default)]
pub struct MeasurementState {
    pub result: Option<Measurement>,
    pub loading: bool,
    pub error: Option<String>,
    pub request_id: Option<crate::worker::protocol::RequestId>,
}

impl MeasurementState {
    pub fn clear(&mut self) {
        self.result = None;
        self.loading = false;
        self.error = None;
        self.request_id = None;
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Exact CAD measurements");
            if state.measurements.loading {
                ui.spinner();
            }
        });
        ui.small(format!(
            "Selection mode: {} · Ctrl+click selects the second entity",
            state.viewer.selection_mode.label()
        ));
        ui.add_space(6.0);

        if let Some(error) = &state.measurements.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return;
        }
        let Some(result) = &state.measurements.result else {
            super::empty_state(
                ui,
                icon::RULER,
                "Select a face, edge or vertex in the 3D view",
            );
            return;
        };

        for (index, entity) in result.entities.iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            entity_ui(ui, state, entity, index + 1);
        }
        if let Some(pair) = &result.pair {
            ui.separator();
            pair_ui(ui, pair);
        }

        ui.separator();
        ui.weak(
            "Topology IDs belong to this render only. Rebuilding or remodeling the object may change them.",
        );
    });
}

fn entity_ui(ui: &mut egui::Ui, state: &AppState, entity: &EntityMeasurement, number: usize) {
    let name = state
        .scene
        .find(&entity.key)
        .map_or(entity.key.as_str(), |node| node.name.as_str());
    ui.strong(format!(
        "{number}. {} {} · {name}",
        kind_label(entity.kind),
        entity.identifier
    ));
    egui::Grid::new(("measurement_entity", number))
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Geometry", &title_case(&entity.geometry_type));
            row(ui, "Part", &(entity.part + 1).to_string());
            row(ui, "Center", &format_point(entity.center));
            if let Some(position) = entity.position {
                row(ui, "Position", &format_point(position));
            }
            if let Some(value) = entity.length {
                row(ui, "Length", &format_number(value));
            }
            if let Some(value) = entity.area {
                row(ui, "Area", &format_number(value));
            }
            if let Some(value) = entity.radius {
                row(ui, "Radius", &format_number(value));
            }
            if let Some(value) = entity.diameter {
                row(ui, "Diameter", &format_number(value));
            }
        });
}

fn pair_ui(ui: &mut egui::Ui, pair: &PairMeasurement) {
    ui.strong("Relationship");
    egui::Grid::new("measurement_pair")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Distance", &format_number(pair.distance));
            row(ui, "Closest point 1", &format_point(pair.first_point));
            row(ui, "Closest point 2", &format_point(pair.second_point));
            if let Some(angle) = pair.angle_degrees {
                let label = match pair.angle_kind.as_deref() {
                    Some("edge_edge") => "Edge angle",
                    Some("face_face") => "Face angle",
                    Some("edge_face") => "Edge/plane angle",
                    _ => "Angle",
                };
                row(ui, label, &format!("{}°", format_number(angle)));
            }
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.monospace(value);
    ui.end_row();
}

fn kind_label(kind: TopologyKind) -> &'static str {
    match kind {
        TopologyKind::Face => "Face",
        TopologyKind::Edge => "Edge",
        TopologyKind::Vertex => "Vertex",
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn format_point(point: [f64; 3]) -> String {
    format!(
        "({}, {}, {})",
        format_number(point[0]),
        format_number(point[1]),
        format_number(point[2])
    )
}

fn format_number(value: f64) -> String {
    let absolute = value.abs();
    if absolute != 0.0 && !(1e-5..1e9).contains(&absolute) {
        return format!("{value:.8e}");
    }
    let text = format!("{value:.9}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::format_number;

    #[test]
    fn numeric_display_keeps_precision_without_noise() {
        assert_eq!(format_number(5.0), "5");
        assert_eq!(format_number(1.2345678912), "1.234567891");
        assert_eq!(format_number(0.000_001), "1.00000000e-6");
    }
}
