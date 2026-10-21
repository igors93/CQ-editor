use super::*;

fn temporary_file(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "cq-editor-document-test-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn detects_line_endings() {
    assert_eq!(LineEnding::detect("a\r\nb"), LineEnding::CrLf);
    assert_eq!(LineEnding::detect("a\nb"), LineEnding::Lf);
    assert_eq!(LineEnding::detect("a\rb"), LineEnding::Cr);
}

#[test]
fn preserves_line_endings_and_tracks_modification() {
    let dir = std::env::temp_dir().join(format!("cq-editor-doc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("script.py");
    std::fs::write(&path, "a = 1\r\nb = 2\r\n").unwrap();

    let mut doc = Document::open(&path).unwrap();
    assert_eq!(doc.text, "a = 1\nb = 2\n");
    assert!(!doc.is_modified());

    doc.text.push_str("c = 3\n");
    assert!(doc.is_modified());
    assert!(doc.save().unwrap());
    assert!(!doc.is_modified());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "a = 1\r\nb = 2\r\nc = 3\r\n"
    );

    std::fs::write(&path, "x = 1\r\n").unwrap();
    let external = doc.disk_version().unwrap().unwrap();
    assert!(doc.reload(external).unwrap());
    assert_eq!(doc.text, "x = 1\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn examples_and_untitled_buffers_need_save_as() {
    let mut example = Document::example("Hello Box", "x = 1\n");
    assert!(!example.save().unwrap());
    assert_eq!(example.display_name(), "Hello Box (example)");
    assert!(!Document::untitled(DEFAULT_SCRIPT).is_modified());
}

#[test]
fn save_as_changes_an_example_into_an_editable_file() {
    let path = temporary_file("example.py");
    let mut doc = Document::example("A Box", "x = 1\r\ny = 2\r\n");
    assert_eq!(doc.text, "x = 1\ny = 2\n");
    doc.text.push_str("z = 3\n");
    doc.save_as(&path).unwrap();

    assert_eq!(doc.path(), Some(path.as_path()));
    assert_eq!(doc.example_title(), None);
    assert_eq!(
        doc.display_name(),
        path.file_name().unwrap().to_string_lossy()
    );
    assert!(!doc.is_modified());
    let eol = LineEnding::NATIVE.as_str();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("x = 1{eol}y = 2{eol}z = 3{eol}")
    );
    assert!(doc.save().unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn opens_and_saves_classic_mac_line_endings() {
    let path = temporary_file("cr.py");
    std::fs::write(&path, "first\rsecond\r").unwrap();
    let mut doc = Document::open(&path).unwrap();
    assert_eq!(doc.line_ending(), LineEnding::Cr);
    assert_eq!(doc.text, "first\nsecond\n");
    doc.text.push_str("third\n");
    assert!(doc.save().unwrap());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "first\rsecond\rthird\r"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reload_refuses_to_discard_unsaved_changes() {
    let path = temporary_file("reload.py");
    std::fs::write(&path, "x = 1\n").unwrap();
    let mut doc = Document::open(&path).unwrap();
    let unchanged = doc.disk_version().unwrap().unwrap();
    assert!(!doc.reload(unchanged).unwrap());
    doc.text = "unsaved change".into();
    assert!(doc.is_modified());
    std::fs::write(&path, "external change\n").unwrap();

    let disk = doc.disk_version().unwrap().unwrap();
    let error = doc.reload(disk.clone()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(doc.text, "unsaved change");
    assert!(doc.is_modified());

    assert!(!doc.disk_matches_saved(&disk));
    assert!(doc.apply_disk_version(disk).unwrap());
    assert_eq!(doc.text, "external change\n");
    assert!(!doc.is_modified());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn classifies_external_updates_without_mutating_the_buffer() {
    let path = temporary_file("external-update.py");
    std::fs::write(&path, "original\n").unwrap();
    let mut doc = Document::open(&path).unwrap();

    let unchanged = doc.disk_version().unwrap().unwrap();
    assert_eq!(
        doc.classify_external_update(&unchanged),
        ExternalUpdate::Unchanged
    );

    std::fs::write(&path, "external\n").unwrap();
    let external = doc.disk_version().unwrap().unwrap();
    assert_eq!(
        doc.classify_external_update(&external),
        ExternalUpdate::Reload
    );
    assert_eq!(doc.text, "original\n");

    doc.text = "local\n".into();
    assert_eq!(
        doc.classify_external_update(&external),
        ExternalUpdate::Conflict
    );
    assert_eq!(doc.text, "local\n");

    std::fs::write(&path, "local\n").unwrap();
    let matching = doc.disk_version().unwrap().unwrap();
    assert_eq!(
        doc.classify_external_update(&matching),
        ExternalUpdate::MatchesBuffer
    );
    assert_eq!(doc.text, "local\n");
    assert!(doc.is_modified());

    std::fs::remove_file(path).unwrap();
}

#[test]
fn recovery_backup_preserves_edits_without_marking_them_saved() {
    let path = temporary_file("recovery.py");
    let recovery_dir = temporary_file("recovery-dir");
    std::fs::write(&path, "original\r\n").unwrap();
    let mut doc = Document::open(&path).unwrap();
    doc.text = "local edit\nsecond line\n".into();

    let backup = doc.create_recovery_backup(&recovery_dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(&backup).unwrap(),
        "local edit\r\nsecond line\r\n"
    );
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(doc.is_modified());

    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir_all(recovery_dir).unwrap();
}

#[test]
fn saving_a_copy_does_not_change_document_identity_or_saved_state() {
    let path = temporary_file("source.py");
    let copy = temporary_file("copy.py");
    std::fs::write(&path, "original\n").unwrap();
    let mut doc = Document::open(&path).unwrap();
    doc.text = "local edit\n".into();

    doc.save_copy(&copy).unwrap();
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "local edit\n");
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(doc.is_modified());
    assert_eq!(
        doc.save_copy(&path).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(copy).unwrap();
}

#[test]
fn applying_a_disk_snapshot_rejects_another_document() {
    let first = temporary_file("first.py");
    let second = temporary_file("second.py");
    std::fs::write(&first, "first\n").unwrap();
    std::fs::write(&second, "second\n").unwrap();
    let snapshot = Document::open(&first)
        .unwrap()
        .disk_version()
        .unwrap()
        .unwrap();
    let mut doc = Document::open(&second).unwrap();

    assert_eq!(
        doc.apply_disk_version(snapshot).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert_eq!(doc.text, "second\n");
    std::fs::remove_file(first).unwrap();
    std::fs::remove_file(second).unwrap();
}

#[test]
fn invalid_utf8_is_replaced_without_preventing_open() {
    let path = temporary_file("invalid.py");
    std::fs::write(&path, [b'a', 0xff, b'\n']).unwrap();
    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.text, "a\u{fffd}\n");
    assert!(!doc.is_modified());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn missing_file_returns_an_error() {
    let path = temporary_file("absent.py");
    assert!(Document::open(&path).is_err());
}
