use super::*;

#[test]
fn drag_motion_is_incremental_instead_of_reapplying_the_whole_drag() {
    let mut drag = Drag {
        mode: DragMode::Pan,
        start: Pos2::new(10.0, 20.0),
        last: Pos2::new(10.0, 20.0),
    };

    assert_eq!(drag.advance(Pos2::new(14.0, 23.0)), Vec2::new(4.0, 3.0));
    assert_eq!(drag.advance(Pos2::new(17.0, 25.0)), Vec2::new(3.0, 2.0));
    assert_eq!(drag.last, Pos2::new(17.0, 25.0));

    let mut incremental = Camera::default();
    incremental.pan(GVec2::new(4.0, 3.0), 600.0);
    incremental.pan(GVec2::new(3.0, 2.0), 600.0);
    let mut once = Camera::default();
    once.pan(GVec2::new(7.0, 5.0), 600.0);
    assert!((incremental.target - once.target).length() < 1e-5);
}

#[test]
fn legacy_mouse_gestures_keep_their_navigation_modes() {
    use egui::{Modifiers, PointerButton};

    let mode = |modifiers, button| {
        requested_drag_mode(
            modifiers,
            button == PointerButton::Primary,
            button == PointerButton::Middle,
            button == PointerButton::Secondary,
        )
    };

    assert!(matches!(
        mode(Modifiers::NONE, PointerButton::Primary),
        Some(DragMode::Orbit)
    ));
    assert!(matches!(
        mode(Modifiers::NONE, PointerButton::Middle),
        Some(DragMode::Pan)
    ));
    assert!(matches!(
        mode(Modifiers::CTRL, PointerButton::Primary),
        Some(DragMode::Pan)
    ));
    assert!(matches!(
        mode(Modifiers::COMMAND, PointerButton::Primary),
        Some(DragMode::Pan)
    ));
    assert!(matches!(
        mode(Modifiers::NONE, PointerButton::Secondary),
        Some(DragMode::Zoom)
    ));
    assert!(matches!(
        mode(Modifiers::SHIFT, PointerButton::Primary),
        Some(DragMode::Zoom)
    ));
}

#[test]
fn zoom_percentage_is_relative_to_the_last_fit_extent() {
    let mut viewer = Viewer::default();
    viewer.camera.extent = 20.0;
    viewer.zoom_reference_extent = 20.0;

    viewer.set_zoom_percent(200.0);
    assert!((viewer.camera.extent - 10.0).abs() < 1e-5);
    assert!((viewer.zoom_percent() - 200.0).abs() < 1e-4);

    viewer.set_zoom_percent(50.0);
    assert!((viewer.camera.extent - 40.0).abs() < 1e-5);
    assert!((viewer.zoom_percent() - 50.0).abs() < 1e-4);
}

#[test]
fn zoom_controls_stay_inside_the_view_and_hide_when_it_is_too_small() {
    let view = Rect::from_min_size(Pos2::new(50.0, 80.0), Vec2::new(800.0, 600.0));
    let controls = zoom_controls_rect(view).unwrap();
    assert!(view.contains_rect(controls));
    assert_eq!(controls.left(), view.left() + ZOOM_CONTROLS_MARGIN);
    assert_eq!(controls.top(), view.top() + ZOOM_CONTROLS_MARGIN);

    let narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, 600.0));
    assert!(zoom_controls_rect(narrow).is_none());
}

#[test]
fn zoom_overlay_buttons_are_interactive_over_the_viewer() {
    fn frame(ctx: &egui::Context, viewer: &mut Viewer, events: Vec<egui::Event>) -> Rect {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut controls = Rect::NOTHING;
        let mut output = ctx.run_ui(input, |ui| {
            let view = ui.available_rect_before_wrap();
            ui.allocate_rect(view, Sense::click_and_drag());
            controls = zoom_controls_rect(view).unwrap();
            viewer.show_zoom_controls(ui, view, controls);
        });
        // This headless test has no renderer to upload the font atlas to.
        output.textures_delta.clear();
        controls
    }

    let ctx = egui::Context::default();
    let mut viewer = Viewer::default();
    let controls = frame(&ctx, &mut viewer, Vec::new());
    // First button: frame margin (6, 5), then a 28-point button.
    let zoom_out = controls.min + Vec2::new(20.0, 19.0);
    frame(
        &ctx,
        &mut viewer,
        vec![
            egui::Event::PointerMoved(zoom_out),
            egui::Event::PointerButton {
                pos: zoom_out,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    frame(
        &ctx,
        &mut viewer,
        vec![egui::Event::PointerButton {
            pos: zoom_out,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert!((viewer.zoom_percent() - 80.0).abs() < 1e-4);
    assert!((viewer.camera.extent - 25.0).abs() < 1e-4);
}

#[test]
fn pressing_the_view_uses_a_closed_hand_cursor_for_navigation() {
    fn frame(
        ctx: &egui::Context,
        viewer: &mut Viewer,
        events: Vec<egui::Event>,
    ) -> egui::CursorIcon {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let scene = Scene::default();
        let settings = crate::settings::Viewer::default();
        let colors = crate::theme::Palette::DARK.viewer();
        let mut output = ctx.run_ui(input, |ui| {
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, Sense::click_and_drag());
            viewer.handle_input(
                ui,
                &response,
                rect,
                zoom_controls_rect(rect),
                &ViewInput {
                    scene: &scene,
                    settings: &settings,
                    colors: &colors,
                    busy: None,
                    empty_hint: None,
                },
                0.0,
            );
        });
        output.textures_delta.clear();
        output.platform_output.cursor_icon
    }

    let ctx = egui::Context::default();
    let mut viewer = Viewer::default();
    frame(&ctx, &mut viewer, Vec::new());
    let position = Pos2::new(400.0, 300.0);
    let cursor = frame(
        &ctx,
        &mut viewer,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );

    assert_eq!(cursor, egui::CursorIcon::Grabbing);
    assert_eq!(
        requested_drag_mode(egui::Modifiers::NONE, false, false, true).map(DragMode::cursor),
        Some(egui::CursorIcon::ResizeVertical)
    );
}

#[test]
fn orbit_changes_only_the_view_direction() {
    let mut camera = Camera::default();
    let before = camera.pose();

    camera.orbit_turntable(GVec2::new(24.0, -12.0));

    assert_eq!(camera.target, before.target);
    assert_eq!(camera.extent, before.extent);
    assert_ne!(camera.rotation, before.rotation);
}
