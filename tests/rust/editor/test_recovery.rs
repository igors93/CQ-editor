use super::*;

fn temporary_directory(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "cq-editor-recovery-test-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn checkpoint_restores_unsaved_work_after_a_simulated_crash() {
    let root = temporary_directory("crash");
    let source = root.join("source.py");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&source, "saved\n").unwrap();
    let mut document = Document::open(&source).unwrap();
    document.text = "unsaved work\n".into();

    let mut running_store = RecoveryStore::new(root.join("store"));
    assert!(running_store.checkpoint(&document).unwrap());
    drop(running_store);

    let restarted_store = RecoveryStore::new(root.join("store"));
    let snapshot = restarted_store.load_session().unwrap().unwrap();
    assert_eq!(snapshot.path.as_deref(), Some(source.as_path()));
    assert_eq!(snapshot.text, "unsaved work\n");
    let recovered = snapshot.into_document();
    assert_eq!(recovered.text, "unsaved work\n");
    assert!(recovered.is_modified());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkpoint_recovers_an_unsaved_untitled_document() {
    let root = temporary_directory("untitled");
    let mut document = Document::untitled("initial\n");
    document.text = "draft without a file\n".into();

    let mut running_store = RecoveryStore::new(root.clone());
    running_store.checkpoint(&document).unwrap();
    drop(running_store);

    let restarted_store = RecoveryStore::new(root.clone());
    let recovered = restarted_store
        .load_session()
        .unwrap()
        .unwrap()
        .into_document();
    assert_eq!(recovered.path(), None);
    assert_eq!(recovered.text, "draft without a file\n");
    assert!(recovered.is_modified());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkpoints_are_deduplicated_and_available_as_local_history() {
    let root = temporary_directory("history");
    let source = root.join("source.py");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&source, "saved\n").unwrap();
    let mut document = Document::open(&source).unwrap();
    let mut store = RecoveryStore::new(root.join("store"));

    document.text = "version one\n".into();
    assert!(store.checkpoint(&document).unwrap());
    assert!(!store.checkpoint(&document).unwrap());
    document.text = "version two\n".into();
    assert!(store.checkpoint(&document).unwrap());

    let history = store.history(Some(&source)).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].text, "version two\n");
    assert!(history.iter().any(|entry| entry.text == "saved\n"));
    assert!(history.iter().any(|entry| entry.text == "version one\n"));
    assert!(history.iter().any(|entry| entry.text == "version two\n"));

    store.clear_session().unwrap();
    assert!(store.load_session().unwrap().is_none());
    assert_eq!(store.history(Some(&source)).unwrap().len(), 3);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_history_is_bounded_per_document() {
    let root = temporary_directory("bounded");
    let source = root.join("source.py");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&source, "saved\n").unwrap();
    let mut document = Document::open(&source).unwrap();
    let mut store = RecoveryStore::new(root.join("store"));

    for version in 0..MAX_HISTORY_PER_DOCUMENT + 5 {
        document.text = format!("version {version}\n");
        assert!(store.checkpoint(&document).unwrap());
    }

    assert_eq!(
        store.history(Some(&source)).unwrap().len(),
        MAX_HISTORY_PER_DOCUMENT
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unmodified_document_clears_only_the_active_session() {
    let root = temporary_directory("clean");
    let source = root.join("source.py");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&source, "saved\n").unwrap();
    let mut document = Document::open(&source).unwrap();
    let mut store = RecoveryStore::new(root.join("store"));

    document.text = "unsaved\n".into();
    store.checkpoint(&document).unwrap();
    let disk = document.disk_version().unwrap().unwrap();
    document.apply_disk_version(disk).unwrap();
    assert!(!store.checkpoint(&document).unwrap());
    assert!(store.load_session().unwrap().is_none());
    assert_eq!(store.history(Some(&source)).unwrap().len(), 2);

    std::fs::remove_dir_all(root).unwrap();
}
