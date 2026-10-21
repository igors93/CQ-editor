//! User preferences, persisted by eframe between sessions.
//!
//! Defaults follow the original CQ-editor where the option existed there.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::worker::protocol::{RunSettings, SceneSettings};

const MAX_RECENT_FILES: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub viewer: Viewer,
    pub editor: Editor,
    pub objects: Objects,
    pub execution: Execution,
    pub recent_files: Vec<PathBuf>,
    /// File reopened on startup.
    pub last_file: Option<PathBuf>,
}

impl Settings {
    pub fn add_recent_file(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    pub fn run_settings(&self) -> RunSettings {
        RunSettings {
            reload_cq: self.execution.reload_cq,
            add_script_dir_to_path: self.execution.add_script_dir_to_path,
            change_working_dir: self.execution.change_working_dir,
            reload_imported_modules: self.execution.reload_imported_modules,
        }
    }

    pub fn scene_settings(&self) -> SceneSettings {
        SceneSettings {
            linear_tolerance: self.viewer.linear_tolerance,
            angular_tolerance: self.viewer.angular_tolerance,
            merge_assemblies: self.objects.merge_assemblies,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: Theme,
    /// Python interpreter with CadQuery; empty means auto-detect.
    pub python: String,
    /// Scale factor on top of the system's.
    pub ui_scale: f32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            python: String::new(),
            ui_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Projection {
    #[default]
    Orthographic,
    Perspective,
}

impl Projection {
    pub const ALL: [Projection; 2] = [Projection::Orthographic, Projection::Perspective];

    pub fn label(self) -> &'static str {
        match self {
            Projection::Orthographic => "Orthographic",
            Projection::Perspective => "Perspective",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OrbitMethod {
    /// Rotation about the world Z axis and the screen's horizontal axis;
    /// Z stays up.
    #[default]
    Turntable,
    /// Free rotation, like rolling a ball under the mouse.
    Trackball,
}

impl OrbitMethod {
    pub const ALL: [OrbitMethod; 2] = [OrbitMethod::Turntable, OrbitMethod::Trackball];

    pub fn label(self) -> &'static str {
        match self {
            OrbitMethod::Turntable => "Turntable",
            OrbitMethod::Trackball => "Trackball",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Viewer {
    pub fit_automatically: bool,
    pub show_navigation_cube: bool,
    pub show_axes_indicator: bool,
    pub use_gradient: bool,
    /// sRGB.
    pub background: [u8; 3],
    /// Bottom color of the gradient, sRGB.
    pub background_aux: [u8; 3],
    pub default_color: [u8; 3],
    pub edge_color: [u8; 3],
    pub show_edges: bool,
    pub edge_width: f32,
    /// Chordal deviation of the mesh relative to the object size.
    pub linear_tolerance: f64,
    /// Radians.
    pub angular_tolerance: f64,
    pub projection: Projection,
    pub orbit_method: OrbitMethod,
    /// Animate view changes (standard views, navigation cube, fit).
    pub animate: bool,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            fit_automatically: true,
            show_navigation_cube: true,
            show_axes_indicator: true,
            use_gradient: false,
            background: [95, 95, 95],
            background_aux: [30, 30, 30],
            default_color: [255, 215, 0],
            edge_color: [0, 0, 0],
            show_edges: true,
            edge_width: 1.5,
            linear_tolerance: 1e-3,
            angular_tolerance: 0.2,
            projection: Projection::Orthographic,
            orbit_method: OrbitMethod::Turntable,
            animate: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Editor {
    pub font_size: f32,
    /// Re-run the script whenever its file changes on disk.
    pub autoreload: bool,
    /// Wait this long after a change, so the file is completely written.
    pub autoreload_delay_ms: u64,
    pub watch_imported_modules: bool,
    pub line_wrap: bool,
    /// Column of the vertical ruler; 0 hides it.
    pub max_line_length: usize,
    pub highlight_current_line: bool,
    pub auto_indent: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            autoreload: false,
            autoreload_delay_ms: 50,
            watch_imported_modules: false,
            line_wrap: false,
            max_line_length: 79,
            highlight_current_line: true,
            auto_indent: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Objects {
    /// Keep visibility and colors of objects across runs (matched by name).
    pub preserve_properties: bool,
    pub clear_before_run: bool,
    /// Show assemblies as one object instead of one per part.
    pub merge_assemblies: bool,
    /// STL export chordal tolerance, model units.
    pub stl_tolerance: f64,
    /// STL export angular tolerance, radians.
    pub stl_angular_tolerance: f64,
}

impl Default for Objects {
    fn default() -> Self {
        Self {
            preserve_properties: false,
            clear_before_run: true,
            merge_assemblies: false,
            stl_tolerance: 0.1,
            stl_angular_tolerance: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Execution {
    pub reload_cq: bool,
    pub add_script_dir_to_path: bool,
    pub change_working_dir: bool,
    pub reload_imported_modules: bool,
}

impl Default for Execution {
    fn default() -> Self {
        Self {
            reload_cq: false,
            add_script_dir_to_path: true,
            change_working_dir: true,
            reload_imported_modules: true,
        }
    }
}

#[cfg(test)]
#[path = "../tests/rust/app/test_settings.rs"]
mod tests;
