//! Variables of the last run, or of the paused frame while debugging.

use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular as icon;

use crate::app::AppState;

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    let filter_id = ui.id().with("variables_filter");
    let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id).unwrap_or_default());

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(icon::FUNNEL_SIMPLE);
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("Filter by name or type")
                        .desired_width(f32::INFINITY),
                );
            });
        });
    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));

    if state.variables.is_empty() {
        super::empty_state(ui, icon::BRACKETS_CURLY, "Variables appear after a run");
        return;
    }

    let needle = filter.to_lowercase();
    let rows: Vec<_> = state
        .variables
        .iter()
        .filter(|v| {
            needle.is_empty()
                || v.name.to_lowercase().contains(&needle)
                || v.type_name.to_lowercase().contains(&needle)
        })
        .collect();

    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .auto_shrink([false, false])
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(120.0).at_least(60.0).clip(true))
        .column(Column::initial(90.0).at_least(50.0).clip(true))
        .column(Column::remainder().clip(true))
        .header(row_height + 2.0, |mut header| {
            for title in ["Name", "Type", "Value"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|body| {
            body.rows(row_height, rows.len(), |mut row| {
                let variable = rows[row.index()];
                row.col(|ui| {
                    ui.label(&variable.name);
                });
                row.col(|ui| {
                    ui.weak(&variable.type_name);
                });
                row.col(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&variable.value).monospace())
                            .truncate(),
                    )
                    .on_hover_text(&variable.value);
                });
            });
        });
}
