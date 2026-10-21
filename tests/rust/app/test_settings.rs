use super::*;

#[test]
fn recent_files_are_unique_and_bounded() {
    let mut settings = Settings::default();
    for i in 0..15 {
        settings.add_recent_file(Path::new(&format!("{i}.py")));
    }
    settings.add_recent_file(Path::new("12.py"));
    assert_eq!(settings.recent_files.len(), MAX_RECENT_FILES);
    assert_eq!(settings.recent_files[0], Path::new("12.py"));
    assert_eq!(
        settings
            .recent_files
            .iter()
            .filter(|p| *p == Path::new("12.py"))
            .count(),
        1
    );
}

#[test]
fn missing_fields_take_defaults() {
    let settings: Settings = serde_json::from_str(r#"{"viewer": {"show_edges": false}}"#).unwrap();
    assert!(!settings.viewer.show_edges);
    assert!(settings.viewer.fit_automatically);
    assert_eq!(settings.editor, Editor::default());
}

#[test]
fn preferences_round_trip_through_json() {
    let mut settings = Settings::default();
    settings.general.theme = Theme::Dark;
    settings.general.python = "python-custom".into();
    settings.viewer.projection = Projection::Perspective;
    settings.viewer.orbit_method = OrbitMethod::Trackball;
    settings.editor.autoreload = true;
    settings.objects.preserve_properties = true;
    settings.execution.reload_imported_modules = false;
    settings.add_recent_file(Path::new("part.py"));

    let json = serde_json::to_string(&settings).unwrap();
    let loaded: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, settings);
}

#[test]
fn execution_and_scene_settings_are_sent_to_worker() {
    let mut settings = Settings::default();
    settings.execution.reload_cq = true;
    settings.execution.add_script_dir_to_path = false;
    settings.execution.change_working_dir = false;
    settings.execution.reload_imported_modules = false;
    settings.viewer.linear_tolerance = 0.005;
    settings.viewer.angular_tolerance = 0.4;
    settings.objects.merge_assemblies = true;

    assert_eq!(
        settings.run_settings(),
        RunSettings {
            reload_cq: true,
            add_script_dir_to_path: false,
            change_working_dir: false,
            reload_imported_modules: false,
        }
    );
    assert_eq!(
        settings.scene_settings(),
        SceneSettings {
            linear_tolerance: 0.005,
            angular_tolerance: 0.4,
            merge_assemblies: true,
        }
    );
}

#[test]
fn reopening_a_recent_file_moves_it_to_the_front() {
    let mut settings = Settings::default();
    settings.add_recent_file(Path::new("first.py"));
    settings.add_recent_file(Path::new("second.py"));
    settings.add_recent_file(Path::new("first.py"));
    assert_eq!(
        settings.recent_files,
        vec![PathBuf::from("first.py"), PathBuf::from("second.py")]
    );
}
