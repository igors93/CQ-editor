use super::*;

#[test]
fn reports_settled_changes_once() {
    let dir = std::env::temp_dir().join(format!("cq-editor-watch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("watched.py");
    std::fs::write(&path, "a").unwrap();

    let mut watcher = FileWatcher::default();
    watcher.set_files(vec![path.clone()]);
    assert!(!watcher.poll(Duration::ZERO));

    std::fs::write(&path, "changed").unwrap();
    watcher.last_poll = None;
    assert!(watcher.poll(Duration::ZERO));
    watcher.last_poll = None;
    assert!(!watcher.poll(Duration::ZERO));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn absent_file_is_waited_for_until_it_is_created() {
    let path =
        std::env::temp_dir().join(format!("cq-editor-watch-created-{}.py", std::process::id()));
    let mut watcher = FileWatcher::default();
    watcher.set_files(vec![path.clone()]);
    assert!(!watcher.poll(Duration::ZERO));
    std::fs::write(&path, "created").unwrap();
    watcher.last_poll = None;
    assert!(watcher.poll(Duration::ZERO));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn replacing_watched_files_keeps_known_stamps() {
    let path =
        std::env::temp_dir().join(format!("cq-editor-watch-stamps-{}.py", std::process::id()));
    std::fs::write(&path, "initial").unwrap();
    let mut watcher = FileWatcher::default();
    watcher.set_files(vec![path.clone()]);
    std::fs::write(&path, "longer contents").unwrap();
    watcher.set_files(vec![path.clone()]);
    assert!(watcher.poll(Duration::ZERO));
    assert_eq!(watcher.watched().collect::<Vec<_>>(), vec![&path]);
    assert_eq!(watcher.next_poll_in(), POLL_INTERVAL);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn clear_stops_watching_and_discards_pending_changes() {
    let mut watcher = FileWatcher::default();
    watcher.set_files(vec![PathBuf::from("missing.py")]);
    watcher.changed_at = Some(Instant::now());
    watcher.clear();
    assert!(!watcher.is_watching());
    assert!(watcher.watched().next().is_none());
    assert!(!watcher.poll(Duration::ZERO));
    assert!(watcher.changed_at.is_none());
}
