use super::*;

#[test]
fn empty_state_rows_are_centered_with_natural_spacing() {
    let ctx = egui::Context::default();
    // A first pass loads the fonts.
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();

    let style = egui::Style::default();
    let text = "Select a workplane in Objects to inspect it";
    let job = empty_state_job(&style, text, egui::Color32::GRAY, 150.0);
    let mut plain = job.clone();
    plain.halign = egui::Align::LEFT;
    let mut justified = plain.clone();
    justified.justify = true;
    let [centered, plain, justified] =
        [job, plain, justified].map(|job| ctx.fonts_mut(|fonts| fonts.layout_job(job)));
    let widths = |galley: &egui::Galley| -> Vec<f32> {
        galley.rows.iter().map(|row| row.rect().width()).collect()
    };

    assert!(centered.rows.len() > 1, "the text wraps in a narrow panel");
    for row in &centered.rows {
        let rect = row.rect();
        assert!(rect.center().x.abs() < 1.0, "row centered: {rect:?}");
    }
    // Each row keeps the width of ordinary text (without its trailing
    // space), where the old justified layout stretched it to the full width.
    let (centered, plain, justified) = (widths(&centered), widths(&plain), widths(&justified));
    for (row, (natural, plain)) in centered.iter().zip(&plain).enumerate() {
        assert!(natural <= plain, "row {row}: {natural} > {plain}");
    }
    assert!(
        centered[0] < justified[0] - 2.0,
        "{centered:?} vs {justified:?}"
    );
}
