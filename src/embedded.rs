//! Files compiled into the binary by `build.rs`: the Python worker package
//! and the example scripts.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// An example script shown in the File > Examples menu.
pub struct Example {
    pub title: String,
    pub source: &'static str,
}

/// The bundled examples, titled after their file names:
/// `03_extruded_profile.py` becomes "Extruded Profile".
pub fn examples() -> Vec<Example> {
    EXAMPLES
        .iter()
        .map(|(file_name, source)| Example {
            title: example_title(file_name),
            source,
        })
        .collect()
}

fn example_title(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".py").unwrap_or(file_name);
    // Drop the "NN_" ordering prefix.
    let stem = match stem.split_once('_') {
        Some((number, rest)) if number.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => stem,
    };
    stem.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Directory that contains the `cq_worker` package, ready to be put on
/// `PYTHONPATH`.
///
/// `CQ_EDITOR_WORKER_DIR` overrides it, and debug builds use the source tree
/// directly so worker edits apply without rebuilding. Otherwise the embedded
/// copy is extracted into `cache_dir`, in a folder named after its content
/// hash so that different versions never mix.
pub fn worker_dir(cache_dir: &Path) -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CQ_EDITOR_WORKER_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let source_tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("worker");
    if cfg!(debug_assertions) && source_tree.join("cq_worker").is_dir() {
        return Ok(source_tree);
    }

    let mut hasher = DefaultHasher::new();
    WORKER_FILES.hash(&mut hasher);
    let dir = cache_dir.join(format!("worker-{:016x}", hasher.finish()));
    let marker = dir.join(".complete");
    if !marker.exists() {
        for (relative, contents) in WORKER_FILES {
            let path = dir.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, contents)?;
        }
        std::fs::write(marker, b"")?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_drop_numbering_and_capitalize() {
        assert_eq!(example_title("01_hello_box.py"), "Hello Box");
        assert_eq!(
            example_title("selectors_and_fillets.py"),
            "Selectors And Fillets"
        );
        assert_eq!(example_title("loft.py"), "Loft");
    }

    #[test]
    fn everything_needed_is_embedded() {
        let names: Vec<_> = WORKER_FILES.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&"cq_worker/__main__.py"));
        assert!(names.contains(&"cq_worker/server.py"));
        assert!(!examples().is_empty());
    }
}
