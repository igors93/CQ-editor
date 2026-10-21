//! The script being edited.
//!
//! Text is kept with `\n` line endings internally; the file's original
//! convention is remembered and restored on save. Files are read and written
//! as UTF-8.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Script shown when no file is open.
pub const DEFAULT_SCRIPT: &str = "import cadquery as cq\n\nresult = cq.Workplane(\"XY\").box(3, 3, 0.5).edges(\"|Z\").fillet(0.125)\nshow_object(result)\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

/// A normalized snapshot read from the document's current path without
/// modifying the editor buffer.
#[derive(Clone, Debug)]
pub struct DiskVersion {
    pub text: String,
    path: PathBuf,
    line_ending: LineEnding,
}

/// Relationship between an on-disk snapshot and the editor's local state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalUpdate {
    /// Only metadata changed; disk contents still match the saved baseline.
    Unchanged,
    /// Disk and buffer now agree, so accepting the snapshot loses nothing.
    MatchesBuffer,
    /// The buffer is clean and can be reloaded automatically.
    Reload,
    /// Disk and buffer diverged from the same saved baseline.
    Conflict,
}

impl LineEnding {
    #[cfg(windows)]
    pub const NATIVE: LineEnding = LineEnding::CrLf;
    #[cfg(not(windows))]
    pub const NATIVE: LineEnding = LineEnding::Lf;

    /// The convention used by `text`, or the platform's for text without
    /// line breaks.
    pub fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            LineEnding::CrLf
        } else if text.contains('\r') {
            LineEnding::Cr
        } else if text.contains('\n') {
            LineEnding::Lf
        } else {
            LineEnding::NATIVE
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }
}

pub struct Document {
    pub text: String,
    path: Option<PathBuf>,
    line_ending: LineEnding,
    saved_hash: u64,
    /// Title of the bundled example this buffer was loaded from. Examples
    /// are read-only: saving one asks for a new location.
    example: Option<String>,
}

impl Document {
    pub fn untitled(text: &str) -> Self {
        let text = normalize(text);
        Self {
            saved_hash: hash(&text),
            text,
            path: None,
            line_ending: LineEnding::NATIVE,
            example: None,
        }
    }

    pub fn example(title: &str, source: &str) -> Self {
        Self {
            example: Some(title.to_owned()),
            ..Self::untitled(source)
        }
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        let (text, line_ending) = read(path)?;
        Ok(Self {
            saved_hash: hash(&text),
            text,
            path: Some(path.to_path_buf()),
            line_ending,
            example: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn example_title(&self) -> Option<&str> {
        self.example.as_deref()
    }

    /// True when the buffer differs from what was last loaded or saved.
    pub fn is_modified(&self) -> bool {
        hash(&self.text) != self.saved_hash
    }

    /// Name for the window title and tabs.
    pub fn display_name(&self) -> String {
        if let Some(title) = &self.example {
            return format!("{title} (example)");
        }
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Saves to the current path; `false` when a "save as" is needed first
    /// (untitled buffers and examples).
    pub fn save(&mut self) -> io::Result<bool> {
        match (&self.path, &self.example) {
            (Some(path), None) => {
                let path = path.clone();
                self.write(&path)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        self.write(path)?;
        self.path = Some(path.to_path_buf());
        self.example = None;
        Ok(())
    }

    /// Applies an inspected disk version when the editor buffer is clean.
    /// Unsaved changes are never discarded by this convenience method;
    /// callers need an explicit conflict flow to replace a modified buffer.
    pub fn reload(&mut self, version: DiskVersion) -> io::Result<bool> {
        if self.is_modified() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "refusing to reload a document with unsaved changes",
            ));
        }
        self.apply_disk_version(version)
    }

    /// Reads the current file without changing the buffer or saved state.
    pub fn disk_version(&self) -> io::Result<Option<DiskVersion>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let (text, line_ending) = read(path)?;
        Ok(Some(DiskVersion {
            text,
            path: path.clone(),
            line_ending,
        }))
    }

    /// Whether a disk snapshot still has the contents last loaded or saved.
    pub fn disk_matches_saved(&self, version: &DiskVersion) -> bool {
        self.path.as_ref() == Some(&version.path) && hash(&version.text) == self.saved_hash
    }

    pub fn classify_external_update(&self, version: &DiskVersion) -> ExternalUpdate {
        if self.disk_matches_saved(version) {
            ExternalUpdate::Unchanged
        } else if version.text == self.text {
            ExternalUpdate::MatchesBuffer
        } else if self.is_modified() {
            ExternalUpdate::Conflict
        } else {
            ExternalUpdate::Reload
        }
    }

    /// Explicitly replaces the buffer with a previously inspected snapshot.
    pub fn apply_disk_version(&mut self, version: DiskVersion) -> io::Result<bool> {
        if self.path.as_ref() != Some(&version.path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "disk snapshot belongs to another document",
            ));
        }
        let changed = version.text != self.text;
        self.saved_hash = hash(&version.text);
        self.text = version.text;
        self.line_ending = version.line_ending;
        Ok(changed)
    }

    /// Writes the current buffer elsewhere without changing this document's
    /// path or marking its edits as saved.
    pub fn save_copy(&self, path: &Path) -> io::Result<()> {
        if self.path.as_deref() == Some(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the copy must use a different path",
            ));
        }
        std::fs::write(path, self.encoded_text())
    }

    /// Creates a uniquely named recovery copy of the current editor buffer.
    /// The document remains modified and keeps its original path.
    pub fn create_recovery_backup(&self, directory: &Path) -> io::Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let stem = self
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("script");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        for suffix in 0..1000 {
            let path = directory.join(format!("{stem}-{timestamp}-{suffix}.recovery.py"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(self.encoded_text().as_bytes())?;
                    file.sync_all()?;
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique recovery file name",
        ))
    }

    fn write(&mut self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.encoded_text())?;
        self.saved_hash = hash(&self.text);
        Ok(())
    }

    fn encoded_text(&self) -> String {
        if self.line_ending == LineEnding::Lf {
            self.text.clone()
        } else {
            self.text.replace('\n', self.line_ending.as_str())
        }
    }
}

fn read(path: &Path) -> io::Result<(String, LineEnding)> {
    let bytes = std::fs::read(path)?;
    let raw = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            log::warn!(
                "{} is not valid UTF-8; invalid bytes were replaced",
                path.display()
            );
            String::from_utf8_lossy(err.as_bytes()).into_owned()
        }
    };
    let line_ending = LineEnding::detect(&raw);
    Ok((normalize(&raw), line_ending))
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[path = "../tests/rust/editor/test_document.rs"]
mod tests;
