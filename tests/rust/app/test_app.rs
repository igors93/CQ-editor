use super::*;

#[test]
fn only_web_links_are_opened() {
    assert!(is_web_url("https://cadquery.readthedocs.io/en/latest/"));
    assert!(is_web_url("HTTP://example.com/model?id=1"));
    // Links from an upload service's reply must not start local programs.
    assert!(!is_web_url(r"C:\Windows\System32\calc.exe"));
    assert!(!is_web_url("file:///etc/passwd"));
    assert!(!is_web_url("javascript:alert(1)"));
    assert!(!is_web_url("https://"));
    assert!(!is_web_url("https://example.com/\nsecond line"));
}

#[test]
fn the_documentation_link_points_to_these_docs() {
    assert!(DOCUMENTATION_URL.ends_with("/docs/README.md"));
    assert!(!DOCUMENTATION_URL.contains("/wiki"));
    assert!(is_web_url(DOCUMENTATION_URL));
}

#[test]
fn links_clicked_in_widgets_are_taken_from_the_platform_output() {
    let ctx = egui::Context::default();
    let mut urls = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.ctx()
            .open_url(egui::OpenUrl::new_tab("https://example.com/a"));
        ui.ctx().copy_text("kept".to_owned());
        urls = take_requested_urls(ui.ctx());
    });
    output.textures_delta.clear();

    assert_eq!(urls, ["https://example.com/a"]);
    // Other commands, such as copying, still reach the platform.
    assert_eq!(
        output.platform_output.commands,
        [egui::OutputCommand::CopyText("kept".to_owned())]
    );
}
