//! Message types exchanged with the Python worker (`worker/cq_worker`).
//!
//! Every message is one JSON object per line. Requests carry a `cmd` tag,
//! worker messages a `type` tag. Mesh arrays travel as base64-encoded
//! little-endian binary and are decoded on the reader thread, off the UI
//! thread.

use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The worker's protocol version this build understands.
pub const PROTOCOL_VERSION: u32 = 1;

pub type RequestId = u64;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Run {
        id: RequestId,
        script: String,
        path: Option<String>,
        debug: bool,
        breakpoints: Vec<usize>,
        settings: RunSettings,
        scene: SceneSettings,
    },
    /// Only valid while a debugging session is paused.
    Debug {
        action: DebugAction,
        breakpoints: Vec<usize>,
    },
    /// Scene keys the GUI no longer shows; lets the worker free them.
    Release {
        keys: Vec<String>,
    },
    Complete {
        id: RequestId,
        source: String,
        /// 1-based.
        line: usize,
        /// 0-based, in characters.
        column: usize,
        path: Option<String>,
        target: CompletionTarget,
    },
    Inspect {
        id: RequestId,
        key: String,
    },
    InspectGeometry {
        id: RequestId,
        key: String,
        frame: usize,
        item: Option<usize>,
        scene: SceneSettings,
    },
    Console {
        id: RequestId,
        source: String,
        scene: SceneSettings,
    },
    ConsoleReset {
        id: RequestId,
    },
    Export {
        id: RequestId,
        keys: Vec<String>,
        format: ExportFormat,
        path: String,
        tolerance: f64,
        angular_tolerance: f64,
    },
    Upload {
        id: RequestId,
        url: String,
        file_type: String,
        keys: Vec<String>,
        source: String,
    },
    CheckUpdates {
        id: RequestId,
    },
    Imports {
        id: RequestId,
        path: String,
    },
    Shutdown,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebugAction {
    Continue,
    Step,
    StepIn,
    StepOut,
    Stop,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionTarget {
    Editor,
    Console,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Step,
    Stl,
    Brep,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 3] = [ExportFormat::Step, ExportFormat::Stl, ExportFormat::Brep];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Step => "step",
            ExportFormat::Stl => "stl",
            ExportFormat::Brep => "brep",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Step => "STEP",
            ExportFormat::Stl => "STL",
            ExportFormat::Brep => "BREP",
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct RunSettings {
    pub reload_cq: bool,
    pub add_script_dir_to_path: bool,
    pub change_working_dir: bool,
    pub reload_imported_modules: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SceneSettings {
    pub linear_tolerance: f64,
    pub angular_tolerance: f64,
    pub merge_assemblies: bool,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Ready(ReadyInfo),
    /// The worker could not start (typically: CadQuery is not installed).
    Fatal {
        message: String,
        #[serde(default)]
        detail: Option<String>,
    },
    Output {
        id: Option<RequestId>,
        stream: OutputStream,
        text: String,
    },
    Log {
        level: String,
        source: String,
        message: String,
    },
    RunFinished(RunFinished),
    DebugPaused {
        id: RequestId,
        line: usize,
        variables: Vec<Variable>,
        objects: Vec<SceneNode>,
    },
    Completions {
        id: RequestId,
        available: bool,
        items: Vec<CompletionItem>,
        signatures: Vec<String>,
    },
    Inspection {
        id: RequestId,
        key: String,
        frames: Vec<StackFrame>,
    },
    InspectionGeometry {
        id: RequestId,
        meshes: Vec<Mesh>,
        plane: Option<Plane>,
    },
    SceneAdd {
        objects: Vec<SceneNode>,
    },
    ConsoleResult {
        id: RequestId,
        more: bool,
    },
    ExportFinished {
        id: RequestId,
        path: String,
    },
    UploadFinished {
        id: RequestId,
        status: u16,
        body: String,
    },
    Updates {
        id: RequestId,
        current: String,
        newer: Vec<String>,
    },
    Imports {
        id: RequestId,
        paths: Vec<String>,
    },
    Error {
        id: Option<RequestId>,
        message: String,
    },
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ReadyInfo {
    pub protocol: u32,
    pub python: String,
    pub cadquery: String,
    pub executable: String,
    pub completion: bool,
}

#[derive(Deserialize, Debug)]
pub struct RunFinished {
    pub id: RequestId,
    pub ok: bool,
    pub debug: bool,
    pub stopped: bool,
    /// `None` when a failed run should keep the current scene.
    pub objects: Option<Vec<SceneNode>>,
    pub variables: Option<Vec<Variable>>,
    pub error: Option<ScriptError>,
    pub duration: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Variable {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ScriptError {
    pub type_name: String,
    pub message: String,
    pub frames: Vec<TracebackFrame>,
    /// Last line of the editor buffer involved, if any.
    pub line: Option<usize>,
    /// Python's own formatting of the traceback.
    pub text: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TracebackFrame {
    pub file: String,
    pub line: usize,
    pub function: String,
    pub code: String,
}

/// Pseudo file name of the editor buffer in worker tracebacks.
pub const SCRIPT_FILENAME: &str = "<cq-editor>";

#[derive(Deserialize, Debug, Clone)]
pub struct CompletionItem {
    pub name: String,
    /// Text to insert at the cursor to complete `name`.
    pub insert: String,
    pub kind: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StackFrame {
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: String,
    pub items: Vec<StackItem>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StackItem {
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: String,
    pub displayable: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Plane {
    pub origin: [f32; 3],
    pub x_dir: [f32; 3],
    pub z_dir: [f32; 3],
    pub size: f32,
}

#[derive(Deserialize, Debug)]
pub struct SceneNode {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub inspectable: bool,
    pub color: Option<[f32; 3]>,
    pub alpha: Option<f32>,
    pub edge_color: Option<[f32; 3]>,
    pub parts: Vec<Part>,
    pub children: Vec<SceneNode>,
}

#[derive(Deserialize, Debug)]
pub struct Part {
    pub mesh: Mesh,
    pub color: Option<[f32; 3]>,
    pub alpha: Option<f32>,
}

/// Triangulated geometry, already decoded.
#[derive(Deserialize, Debug, Default)]
pub struct Mesh {
    #[serde(deserialize_with = "vec3_f32")]
    pub positions: Vec<[f32; 3]>,
    #[serde(deserialize_with = "vec3_f32")]
    pub normals: Vec<[f32; 3]>,
    #[serde(deserialize_with = "u32_array")]
    pub indices: Vec<u32>,
    /// Line segments, two points each.
    #[serde(deserialize_with = "vec3_f32")]
    pub edges: Vec<[f32; 3]>,
    #[serde(deserialize_with = "vec3_f32")]
    pub points: Vec<[f32; 3]>,
    pub bbox: [[f32; 3]; 2],
}

fn base64_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(text.as_bytes())
        .map_err(serde::de::Error::custom)
}

fn vec3_f32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<[f32; 3]>, D::Error> {
    let bytes = base64_bytes(deserializer)?;
    if bytes.len() % 12 != 0 {
        return Err(serde::de::Error::custom(
            "vector array length is not a multiple of 12",
        ));
    }
    Ok(bytes
        .as_chunks::<12>()
        .0
        .iter()
        .map(|c| {
            let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
            [f(0), f(4), f(8)]
        })
        .collect())
}

fn u32_array<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u32>, D::Error> {
    let bytes = base64_bytes(deserializer)?;
    if bytes.len() % 4 != 0 {
        return Err(serde::de::Error::custom(
            "index array length is not a multiple of 4",
        ));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[cfg(test)]
#[path = "../../tests/rust/protocol/test_protocol.rs"]
mod tests;
