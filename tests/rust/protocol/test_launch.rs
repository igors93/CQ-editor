use super::*;

#[test]
fn explicit_interpreter_overrides_auto_detection() {
    assert_eq!(
        find_python(Some("  custom/python  ")),
        PathBuf::from("custom/python")
    );
    assert_eq!(
        find_python(Some("C:/tools/python.exe")),
        PathBuf::from("C:/tools/python.exe")
    );
}

#[test]
fn existing_interpreter_requires_a_regular_file() {
    let directory =
        std::env::temp_dir().join(format!("cq-editor-launch-test-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("bin")).unwrap();
    let python = directory.join("bin").join("python-test");
    assert_eq!(existing(&directory, &["bin", "python-test"]), None);
    std::fs::write(&python, b"").unwrap();
    assert_eq!(existing(&directory, &["bin", "python-test"]), Some(python));
    assert_eq!(existing(&directory, &["bin"]), None);
    std::fs::remove_dir_all(directory).unwrap();
}
