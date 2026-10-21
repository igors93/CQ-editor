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
    assert!(doc.reload().unwrap());
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
fn reload_reports_unchanged_content_and_clears_modified_state() {
    let path = temporary_file("reload.py");
    std::fs::write(&path, "x = 1\n").unwrap();
    let mut doc = Document::open(&path).unwrap();
    assert!(!doc.reload().unwrap());
    doc.text = "unsaved change".into();
    assert!(doc.is_modified());
    assert!(doc.reload().unwrap());
    assert_eq!(doc.text, "x = 1\n");
    assert!(!doc.is_modified());
    std::fs::remove_file(path).unwrap();
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
