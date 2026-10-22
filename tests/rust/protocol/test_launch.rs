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
fn conda_environments_are_listed_after_the_base_interpreter() {
    let root = std::env::temp_dir().join(format!("cq-editor-conda-test-{}", std::process::id()));
    let interpreter = |environment: &Path| {
        CONDA_PYTHON
            .iter()
            .fold(environment.to_path_buf(), |path, part| path.join(part))
    };
    for environment in [
        root.clone(),
        root.join("envs").join("cadquery"),
        root.join("envs").join("broken"),
    ] {
        std::fs::create_dir_all(interpreter(&environment).parent().unwrap()).unwrap();
    }
    std::fs::write(interpreter(&root), b"").unwrap();
    std::fs::write(interpreter(&root.join("envs").join("cadquery")), b"").unwrap();

    assert_eq!(
        environments_in(&root),
        [
            interpreter(&root),
            interpreter(&root.join("envs").join("cadquery"))
        ]
    );
    assert!(environments_in(&root.join("missing")).is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn candidates_exist_and_are_listed_once() {
    let found = candidates();
    assert!(found.iter().all(|path| path.is_file()));
    let unique: std::collections::BTreeSet<_> = found.iter().collect();
    assert_eq!(unique.len(), found.len());
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
