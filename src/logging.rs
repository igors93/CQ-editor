//! Application-wide log shown by the Log panel.
//!
//! Records from the `log` crate, the worker's stderr, Python logging and
//! script `print`/`log()` output all land in one bounded buffer. Multi-line
//! messages are split into one entry per line so the panel can use fixed-height
//! rows.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// Oldest entries are dropped beyond this many lines.
const MAX_ENTRIES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }

    /// Maps Python logging level names.
    pub fn from_python(name: &str) -> Self {
        match name {
            "critical" | "error" => Level::Error,
            "warning" => Level::Warn,
            "debug" => Level::Debug,
            _ => Level::Info,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub time: String,
    pub level: Level,
    pub source: String,
    pub text: String,
}

#[derive(Default)]
struct Store {
    entries: VecDeque<Entry>,
    /// Increments on every change so views can cheaply detect updates.
    revision: u64,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(Mutex::default)
}

/// Appends a message, stripping terminal escape sequences.
pub fn push(level: Level, source: &str, message: &str) {
    let time = chrono::Local::now().format("%H:%M:%S").to_string();
    let message = strip_ansi(message);
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    for line in message.trim_end_matches('\n').split('\n') {
        store.entries.push_back(Entry {
            time: time.clone(),
            level,
            source: source.to_owned(),
            text: line.trim_end_matches('\r').to_owned(),
        });
    }
    while store.entries.len() > MAX_ENTRIES {
        store.entries.pop_front();
    }
    store.revision += 1;
}

pub fn clear() {
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    store.entries.clear();
    store.revision += 1;
}

pub fn revision() -> u64 {
    store().lock().unwrap_or_else(|e| e.into_inner()).revision
}

/// Calls `f` with the current entries, oldest first.
pub fn with_entries<R>(f: impl FnOnce(&VecDeque<Entry>) -> R) -> R {
    f(&store().lock().unwrap_or_else(|e| e.into_inner()).entries)
}

/// Removes ANSI escape sequences such as color codes (`ESC [ ... letter`).
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // Parameter and intermediate bytes, then one final byte in @..~.
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Our own records down to debug; libraries (wgpu, winit) only when
        // something is wrong.
        metadata.target().starts_with("cq_editor") || metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = match record.level() {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            log::Level::Info => Level::Info,
            log::Level::Debug | log::Level::Trace => Level::Debug,
        };
        let message = record.args().to_string();
        eprintln!("[{}] {}: {message}", level.label(), record.target());
        // The Vulkan loader reports every stale layer registered on the
        // system (e.g. of uninstalled overlays); that is noise for users.
        let loader_noise = record.target().starts_with("wgpu_hal::vulkan::instance");
        if level != Level::Debug && !loader_noise {
            push(level, "app", &message);
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    static LOGGER: Logger = Logger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

#[cfg(test)]
#[path = "../tests/rust/app/test_logging.rs"]
mod tests;
