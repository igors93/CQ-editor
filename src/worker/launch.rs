//! Finding a Python interpreter that has CadQuery.

use std::path::{Path, PathBuf};

#[cfg(windows)]
const VENV_PYTHON: &[&str] = &["Scripts", "python.exe"];
#[cfg(not(windows))]
const VENV_PYTHON: &[&str] = &["bin", "python3"];

#[cfg(windows)]
const CONDA_PYTHON: &[&str] = &["python.exe"];
#[cfg(not(windows))]
const CONDA_PYTHON: &[&str] = &["bin", "python3"];

#[cfg(windows)]
const PATH_PYTHON: &str = "python";
#[cfg(not(windows))]
const PATH_PYTHON: &str = "python3";

/// Picks the interpreter, in order of preference:
///
/// 1. `explicit` (command line or preferences), when not empty;
/// 2. the `CQ_EDITOR_PYTHON` environment variable;
/// 3. the active virtual or conda environment (`VIRTUAL_ENV`, `CONDA_PREFIX`);
/// 4. a `.venv` next to the executable, in the working directory or (for
///    development builds) in the source tree;
/// 5. `python`/`python3` from `PATH`.
pub fn find_python(explicit: Option<&str>) -> PathBuf {
    if let Some(path) = explicit.map(str::trim).filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("CQ_EDITOR_PYTHON") {
        return PathBuf::from(path);
    }

    let environments = [
        std::env::var_os("VIRTUAL_ENV").map(|p| (PathBuf::from(p), VENV_PYTHON)),
        std::env::var_os("CONDA_PREFIX").map(|p| (PathBuf::from(p), CONDA_PYTHON)),
    ];
    for (root, relative) in environments.into_iter().flatten() {
        if let Some(python) = existing(&root, relative) {
            return python;
        }
    }

    let mut roots = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        roots.push(dir);
    }
    if let Ok(dir) = std::env::current_dir() {
        roots.push(dir);
    }
    if cfg!(debug_assertions) {
        roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    }
    for root in roots {
        if let Some(python) = existing(&root.join(".venv"), VENV_PYTHON) {
            return python;
        }
    }

    PathBuf::from(PATH_PYTHON)
}

fn existing(root: &Path, relative: &[&str]) -> Option<PathBuf> {
    let path = relative
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part));
    path.is_file().then_some(path)
}

#[cfg(test)]
#[path = "../../tests/rust/protocol/test_launch.rs"]
mod tests;
