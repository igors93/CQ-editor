//! Crash recovery and bounded local document history.
//!
//! The active unsaved buffer is stored separately from history so startup can
//! distinguish a recoverable interrupted session. Every file is written with
//! the same atomic, synchronized primitive used by normal document saves.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::document::{Document, LineEnding, atomic_create, atomic_write};

const SCHEMA_VERSION: u32 = 1;
pub const CHECKPOINT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
pub const MAX_HISTORY_PER_DOCUMENT: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoverySnapshot {
    schema: u32,
    pub timestamp_ms: u64,
    pub path: Option<PathBuf>,
    pub text: String,
    pub line_ending: LineEnding,
    pub utf8_bom: bool,
}

impl RecoverySnapshot {
    fn capture(document: &Document, timestamp_ms: u64) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            timestamp_ms,
            path: document.path().map(Path::to_path_buf),
            text: document.text.clone(),
            line_ending: document.line_ending(),
            utf8_bom: document.has_utf8_bom(),
        }
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "untitled".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
    }

    pub fn into_document(self) -> Document {
        Document::recovered(self.text, self.path, self.line_ending, self.utf8_bom)
    }

    fn validate(self) -> io::Result<Self> {
        if self.schema != SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported recovery format version {}", self.schema),
            ));
        }
        Ok(self)
    }
}

pub struct RecoveryStore {
    root: PathBuf,
    last_content: Option<u64>,
    last_timestamp_ms: u64,
    session_active: bool,
}

impl RecoveryStore {
    pub fn new(root: PathBuf) -> Self {
        let session_active = root.join("active-session.json").is_file();
        Self {
            root,
            last_content: None,
            last_timestamp_ms: 0,
            session_active,
        }
    }

    fn session_path(&self) -> PathBuf {
        self.root.join("active-session.json")
    }

    fn history_dir(&self) -> PathBuf {
        self.root.join("history")
    }

    pub fn load_session(&self) -> io::Result<Option<RecoverySnapshot>> {
        let path = self.session_path();
        match std::fs::read(&path) {
            Ok(bytes) => decode(&bytes).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Persists a modified buffer as the active session and as a history
    /// version. Repeated identical contents do not create duplicate versions.
    pub fn checkpoint(&mut self, document: &Document) -> io::Result<bool> {
        if !document.is_modified() {
            self.clear_session()?;
            return Ok(false);
        }

        if self.last_content.is_none()
            && let Some(path) = document.path()
            && let Err(error) = self.archive_path(path)
        {
            log::warn!(
                "Cannot archive the saved baseline {}: {error}",
                path.display()
            );
        }

        let timestamp = self.next_timestamp();
        let snapshot = RecoverySnapshot::capture(document, timestamp);
        let content_key = content_key(&snapshot);
        if self.last_content == Some(content_key) {
            return Ok(false);
        }

        std::fs::create_dir_all(self.history_dir())?;
        let encoded = serde_json::to_vec(&snapshot).map_err(io::Error::other)?;
        atomic_write(&self.session_path(), &encoded)?;
        self.session_active = true;
        self.append_history(&snapshot, &encoded)?;
        self.prune_history(snapshot.path.as_deref())?;
        self.last_content = Some(content_key);
        self.last_timestamp_ms = timestamp;
        Ok(true)
    }

    /// Adds the current document state to history without marking it as an
    /// interrupted session. Used to retain saved baselines and Save As targets.
    pub fn archive(&mut self, document: &Document) -> io::Result<bool> {
        let mut snapshot = RecoverySnapshot::capture(document, 0);
        if self
            .history(snapshot.path.as_deref())?
            .first()
            .is_some_and(|latest| content_key(latest) == content_key(&snapshot))
        {
            return Ok(false);
        }
        snapshot.timestamp_ms = self.next_timestamp();
        std::fs::create_dir_all(self.history_dir())?;
        let encoded = serde_json::to_vec(&snapshot).map_err(io::Error::other)?;
        self.append_history(&snapshot, &encoded)?;
        self.prune_history(snapshot.path.as_deref())?;
        self.last_timestamp_ms = snapshot.timestamp_ms;
        Ok(true)
    }

    pub fn archive_path(&mut self, path: &Path) -> io::Result<bool> {
        match Document::open(path) {
            Ok(document) => self.archive(&document),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn clear_session(&mut self) -> io::Result<()> {
        if !self.session_active {
            self.last_content = None;
            return Ok(());
        }
        match std::fs::remove_file(self.session_path()) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.last_content = None;
        self.session_active = false;
        Ok(())
    }

    pub fn history(&self, path: Option<&Path>) -> io::Result<Vec<RecoverySnapshot>> {
        let mut versions = self.history_files()?;
        versions.retain(|(_, snapshot)| snapshot.path.as_deref() == path);
        versions.sort_by_key(|(_, snapshot)| std::cmp::Reverse(snapshot.timestamp_ms));
        Ok(versions.into_iter().map(|(_, snapshot)| snapshot).collect())
    }

    fn append_history(&self, snapshot: &RecoverySnapshot, encoded: &[u8]) -> io::Result<()> {
        let history = self.history_dir();
        for suffix in 0..1000 {
            let path = history.join(format!("version-{}-{suffix}.json", snapshot.timestamp_ms));
            match atomic_create(&path, encoded) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique history version name",
        ))
    }

    fn prune_history(&self, document_path: Option<&Path>) -> io::Result<()> {
        let mut versions = self.history_files()?;
        versions.retain(|(_, snapshot)| snapshot.path.as_deref() == document_path);
        versions.sort_by_key(|(_, snapshot)| std::cmp::Reverse(snapshot.timestamp_ms));
        for (path, _) in versions.into_iter().skip(MAX_HISTORY_PER_DOCUMENT) {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn history_files(&self) -> io::Result<Vec<(PathBuf, RecoverySnapshot)>> {
        let entries = match std::fs::read_dir(self.history_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut versions = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read(&path).and_then(|bytes| decode(&bytes)) {
                Ok(snapshot) => versions.push((path, snapshot)),
                Err(error) => {
                    log::warn!("Cannot read recovery history {}: {error}", path.display())
                }
            }
        }
        Ok(versions)
    }

    fn next_timestamp(&self) -> u64 {
        timestamp_ms().max(self.last_timestamp_ms.saturating_add(1))
    }
}

fn decode(bytes: &[u8]) -> io::Result<RecoverySnapshot> {
    serde_json::from_slice::<RecoverySnapshot>(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .validate()
}

fn content_key(snapshot: &RecoverySnapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    snapshot.path.hash(&mut hasher);
    snapshot.text.hash(&mut hasher);
    snapshot.line_ending.as_str().hash(&mut hasher);
    snapshot.utf8_bom.hash(&mut hasher);
    hasher.finish()
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../tests/rust/editor/test_recovery.rs"]
mod tests;
