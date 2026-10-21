//! The script being edited.
//!
//! Text is kept with `\n` line endings internally; the file's original
//! convention is remembered and restored on save. Files are read and written
//! as UTF-8.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

/// Script shown when no file is open.
pub const DEFAULT_SCRIPT: &str = "import cadquery as cq\n\nresult = cq.Workplane(\"XY\").box(3, 3, 0.5).edges(\"|Z\").fillet(0.125)\nshow_object(result)\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
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

    /// Re-reads the file after an external change. Returns `true` if the
    /// contents differ from the buffer.
    pub fn reload(&mut self) -> io::Result<bool> {
        let Some(path) = &self.path else {
            return Ok(false);
        };
        let (text, line_ending) = read(path)?;
        let changed = text != self.text;
        self.saved_hash = hash(&text);
        self.text = text;
        self.line_ending = line_ending;
        Ok(changed)
    }

    fn write(&mut self, path: &Path) -> io::Result<()> {
        let text = if self.line_ending == LineEnding::Lf {
            self.text.clone()
        } else {
            self.text.replace('\n', self.line_ending.as_str())
        };
        std::fs::write(path, text)?;
        self.saved_hash = hash(&self.text);
        Ok(())
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
mod tests {
    use super::*;

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
}
