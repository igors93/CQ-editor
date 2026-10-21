//! Application state and the frame loop.
//!
//! Each frame: apply settings, drain worker events, poll the file watcher and
//! collect keyboard shortcuts; then build the UI (menu, toolbar, dock area,
//! status bar, dialogs), which only *queues* [`Action`]s; finally execute the
//! queued actions. Keeping mutations out of the widget code makes the data
//! flow easy to follow.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::ViewportCommand;
use egui_dock::{DockArea, DockState};

use crate::commands::{self, Action};
use crate::dialogs::{
    self, Dialogs, ExternalChange, ExternalChangeChoice, HistoryDialog, Intent, UnsavedChoice,
};
use crate::document::{DEFAULT_SCRIPT, Document, ExternalUpdate};
use crate::editor::{CodeEditor, CompletionRequest, EditorOptions};
use crate::embedded::{self, Example};
use crate::file_watcher::FileWatcher;
use crate::logging::{self, Level};
use crate::panels::{
    self, ConsoleState, EntryKind, InspectorState, LogView, MeasurementState, Tab, TabContext,
};
use crate::recovery::{CHECKPOINT_INTERVAL, RecoverySnapshot, RecoveryStore};
use crate::scene::{Grid, Mesh, NodeKey, Overlay, Rgb, Scene, SceneNode};
use crate::settings::{self, Projection, Settings};
use crate::theme::{self, Palette};
use crate::viewer::{ViewInput, Viewer, ViewerEvent};
use crate::worker::protocol::{
    CompletionTarget, DebugAction, ExportFormat, Message, OutputStream, Request, RequestId,
    RunFinished, SceneNode as NodeMessage, TopologyKind, TopologyReference, Variable,
};
use crate::worker::{self, Event, LaunchConfig, Worker, launch};

const SETTINGS_KEY: &str = "settings";
const LAYOUT_KEY: &str = "dock_layout";
const LOGO_ICO: &[u8] = include_bytes!("../assets/icons/cadquery_logo_dark.ico");

/// Command-line options.
#[derive(Debug, Default)]
pub struct StartupOptions {
    pub file: Option<PathBuf>,
    pub python: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}

pub struct Status {
    pub kind: StatusKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Running { id: RequestId, debug: bool },
    Paused { id: RequestId },
}

/// What an outstanding request was for, so replies and errors can be routed.
#[derive(Clone, Debug)]
enum Pending {
    Run,
    Completion,
    ConsoleCompletion,
    Inspect,
    InspectGeometry,
    Console,
    Export,
    Upload,
    Updates,
    Imports,
    Measure,
}

pub struct AppState {
    pub settings: Settings,
    pub document: Document,
    pub editor: CodeEditor,
    pub scene: Scene,
    pub viewer: Viewer,
    pub worker: Worker,
    pub variables: Vec<Variable>,
    pub traceback: Option<crate::worker::protocol::ScriptError>,
    pub inspector: InspectorState,
    pub measurements: MeasurementState,
    pub console: ConsoleState,
    pub log_view: LogView,
    pub status: Option<Status>,
    pub dialogs: Dialogs,
    ctx: egui::Context,
    phase: Phase,
    /// A render was requested while one was running.
    rerun: bool,
    /// Render as soon as the worker is ready (`Some(debug)`).
    run_when_ready: Option<bool>,
    pending: HashMap<RequestId, Pending>,
    actions: Vec<Action>,
    watcher: FileWatcher,
    watch_key: Option<(bool, bool, Option<PathBuf>)>,
    examples: Vec<Example>,
    palette: Palette,
    cache_dir: PathBuf,
    recovery: RecoveryStore,
    next_recovery: Instant,
    recovery_error_reported: bool,
    python_override: Option<String>,
    worker_was_ready: bool,
    allow_close: bool,
    title: String,
    applied_theme: Option<settings::Theme>,
    applied_scale: f32,
    logo: Option<egui::TextureHandle>,
}

impl AppState {
    pub fn queue(&mut self, actions: impl IntoIterator<Item = Action>) {
        self.actions.extend(actions);
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn examples(&self) -> &[Example] {
        &self.examples
    }

    pub fn default_color(&self) -> Rgb {
        self.settings
            .viewer
            .default_color
            .map(|c| f32::from(c) / 255.0)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.phase, Phase::Paused { .. })
    }

    pub fn is_debugging(&self) -> bool {
        matches!(
            self.phase,
            Phase::Paused { .. } | Phase::Running { debug: true, .. }
        )
    }

    pub fn is_busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// The CadQuery logo, loaded on first use.
    pub fn logo(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.logo.is_none() {
            let image =
                image::load_from_memory_with_format(LOGO_ICO, image::ImageFormat::Ico).ok()?;
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            self.logo =
                Some(ctx.load_texture("cadquery-logo", color_image, egui::TextureOptions::LINEAR));
        }
        self.logo.clone()
    }

    fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status = Some(Status {
            kind,
            text: text.into(),
        });
    }

    fn now(&self) -> f64 {
        self.ctx.input(|i| i.time)
    }

    // -- per-frame bookkeeping ------------------------------------------------

    fn begin_frame(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.general.theme) {
            theme::apply(ctx, self.settings.general.theme);
            self.applied_theme = Some(self.settings.general.theme);
        }
        // egui's own Ctrl +/- zoom and the preference stay in sync.
        if self.settings.general.ui_scale != self.applied_scale {
            ctx.set_zoom_factor(self.settings.general.ui_scale);
            self.applied_scale = self.settings.general.ui_scale;
        } else if (ctx.zoom_factor() - self.applied_scale).abs() > 1e-3 {
            self.applied_scale = ctx.zoom_factor();
            self.settings.general.ui_scale = self.applied_scale;
        }
        self.palette = Palette::of(ctx.theme());

        self.process_worker_events();
        self.sync_watcher();
        self.poll_watcher(ctx);
        self.poll_recovery(ctx);
        commands::collect_shortcuts(ctx, &mut self.actions);
    }

    fn end_frame(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.document.is_modified()
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialogs.unsaved = Some(Intent::Quit);
        }

        let marker = if self.document.is_modified() { "*" } else { "" };
        let folder = self
            .document
            .path()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .map(|name| format!(" — {}", name.to_string_lossy()))
            .unwrap_or_default();
        let title = format!(
            "{}{marker}{folder} — CQ-editor",
            self.document.display_name()
        );
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    // -- panels ---------------------------------------------------------------

    pub fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let colors = self.palette.editor();
        let options = EditorOptions {
            font_size: self.settings.editor.font_size,
            line_wrap: self.settings.editor.line_wrap,
            ruler: self.settings.editor.max_line_length,
            highlight_current_line: self.settings.editor.highlight_current_line,
            auto_indent: self.settings.editor.auto_indent,
            colors: &colors,
        };
        let output = self.editor.ui(ui, &mut self.document, &options);
        if let Some(request) = output.completion_request {
            self.request_completion(request);
        }
    }

    pub fn viewer_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let colors = self.palette.viewer();
        let busy = match (self.phase, self.worker.status()) {
            (Phase::Running { debug: false, .. }, _) => Some("Running script…"),
            (Phase::Running { debug: true, .. }, _) => Some("Debugging…"),
            (Phase::Idle, worker::Status::Starting) => Some("Starting Python…"),
            _ => None,
        };
        let hint = self
            .worker
            .is_ready()
            .then_some("Press F5 to render the script");
        let events = self.viewer.ui(
            ui,
            frame,
            ViewInput {
                scene: &self.scene,
                settings: &self.settings.viewer,
                colors: &colors,
                busy,
                empty_hint: hint,
            },
        );
        for event in events {
            match event {
                ViewerEvent::Picked {
                    key,
                    topology,
                    extend,
                } => self.actions.push(Action::Select {
                    key,
                    topology,
                    extend,
                }),
                ViewerEvent::FocusRequested => self.actions.push(Action::FitSelection),
                ViewerEvent::Screenshot(Ok(path)) => {
                    self.set_status(
                        StatusKind::Success,
                        format!("Screenshot saved to {}", path.display()),
                    );
                }
                ViewerEvent::Screenshot(Err(err)) => self.dialogs.error("Screenshot failed", err),
            }
        }
    }

    // -- actions ----------------------------------------------------------------

    fn execute(&mut self, action: Action) {
        let animate = self.settings.viewer.animate;
        let now = self.now();
        match action {
            Action::NewFile => self.guard_unsaved(Intent::New),
            Action::OpenFile => self.guard_unsaved(Intent::Open),
            Action::OpenPath(path) => self.guard_unsaved(Intent::OpenPath(path)),
            Action::OpenExample(index) => self.guard_unsaved(Intent::OpenExample(index)),
            Action::Save => {
                self.save();
            }
            Action::SaveAs => {
                self.save_as();
            }
            Action::LocalHistory => self.show_local_history(),
            Action::ToggleAutoreload => {
                self.settings.editor.autoreload = !self.settings.editor.autoreload
            }
            Action::Quit => self.guard_unsaved(Intent::Quit),

            Action::ToggleComment => self.editor.toggle_comment(&mut self.document),
            Action::Autocomplete => self.editor.request_completion(),
            Action::Find => self.editor.open_search(&self.document),
            Action::FindNext | Action::FindPrevious => {
                if self.editor.search_is_open() {
                    self.editor
                        .find_next(&self.document, action == Action::FindNext);
                } else {
                    self.editor.open_search(&self.document);
                }
            }
            Action::Preferences => self.dialogs.preferences = true,

            Action::Render => self.start_run(false),
            Action::ToggleDebug => {
                if self.is_debugging() {
                    self.stop_debugging();
                } else {
                    self.start_run(true);
                }
            }
            Action::Debug(action) => self.debug_command(action),
            Action::Stop => self.stop(),
            Action::ClearConsole => self.console.clear(),
            Action::ClearLog => logging::clear(),
            Action::RestartPython => self.start_worker(),

            Action::Fit => self.viewer.fit(&self.scene, animate, now),
            Action::FitSelection => self.viewer.fit_selection(&self.scene, animate, now),
            Action::View(view) => self.viewer.set_standard_view(view, animate, now),
            Action::Display(mode) => self.viewer.display_mode = mode,
            Action::SelectionMode(mode) => self.viewer.selection_mode = mode,
            Action::ToggleProjection => {
                self.settings.viewer.projection = match self.settings.viewer.projection {
                    Projection::Orthographic => Projection::Perspective,
                    Projection::Perspective => Projection::Orthographic,
                };
            }
            // Layout changes are handled by the app, which owns the dock.
            Action::TogglePanel(_) | Action::ResetLayout => {}

            Action::Export(format) => self.export(format),
            Action::ToggleInspect => self.toggle_inspect(),
            Action::Screenshot => self.screenshot(),
            Action::Upload => {
                self.dialogs.upload.get_or_insert_with(Default::default);
            }

            Action::Documentation => open_url("https://github.com/CadQuery/CQ-editor/wiki"),
            Action::CadQueryDocumentation => open_url("https://cadquery.readthedocs.io/en/latest/"),
            Action::About => self.dialogs.about = true,
            Action::CheckUpdates => {
                if self.send_tracked(Pending::Updates, |id| Request::CheckUpdates { id }) {
                    self.set_status(StatusKind::Info, "Checking for CadQuery updates…");
                }
            }

            Action::GoToLine(line) => self.editor.go_to_line(&self.document.text, line),
            Action::Select {
                key,
                topology,
                extend,
            } => self.select(key, topology, extend),
            Action::SetVisible { key, visible } => self.scene.set_visible(&key, visible),
            Action::RemoveSelected => {
                let keys = self.scene.selected_top_level();
                let removed = self.scene.remove_top_level(&keys);
                self.release(removed);
                self.request_measurement();
                self.refresh_inspector();
            }
            Action::ClearObjects => {
                let removed = self.scene.clear();
                self.release(removed);
                self.measurements.clear();
                self.scene.overlay = Overlay::default();
                self.refresh_inspector();
            }
            Action::InspectEntry { frame, item } => {
                self.inspector.selected = Some((frame, item));
                if self.inspector.show_in_view {
                    self.request_inspection_geometry();
                }
            }
            Action::ConsoleSubmit(source) => self.console_submit(source),
            Action::ConsoleComplete => self.console_complete(),
            Action::ResetConsole => {
                if self.send_tracked(Pending::Console, |id| Request::ConsoleReset { id }) {
                    self.console.more = false;
                    self.console.push(EntryKind::Info, "Console restarted\n");
                }
            }
            Action::StartUpload { url, file_type } => self.start_upload(url, file_type),
        }
    }

    fn guard_unsaved(&mut self, intent: Intent) {
        if self.document.is_modified() {
            self.dialogs.unsaved = Some(intent);
        } else {
            self.perform(intent);
        }
    }

    pub fn resolve_unsaved(&mut self, intent: Intent, choice: UnsavedChoice) {
        match choice {
            UnsavedChoice::Save => {
                if self.save() {
                    self.perform(intent);
                }
            }
            UnsavedChoice::Discard => self.perform(intent),
            UnsavedChoice::Cancel => {}
        }
    }

    pub fn resolve_recovery_session(&mut self, snapshot: RecoverySnapshot, restore: bool) {
        if let Err(error) = self.recovery.clear_session() {
            self.dialogs.error(
                "Cannot update recovery state",
                format!("Your recovery was kept on disk.\n\n{error}"),
            );
            self.dialogs.recovery_session = Some(snapshot);
            return;
        }
        if restore {
            let name = snapshot.display_name();
            self.set_document(snapshot.into_document());
            self.checkpoint_recovery();
            self.set_status(
                StatusKind::Success,
                format!("Recovered unsaved work for {name}"),
            );
        } else {
            self.set_status(
                StatusKind::Info,
                "Discarded the interrupted recovery session",
            );
        }
    }

    pub fn restore_history(&mut self, snapshot: RecoverySnapshot) {
        self.checkpoint_recovery();
        let name = snapshot.display_name();
        self.set_document(snapshot.into_document());
        self.checkpoint_recovery();
        self.set_status(
            StatusKind::Success,
            format!("Restored a local history version of {name}"),
        );
    }

    pub fn resolve_external_change(
        &mut self,
        change: ExternalChange,
        choice: ExternalChangeChoice,
    ) {
        if self.document.path() != Some(change.path.as_path()) {
            return;
        }
        match choice {
            ExternalChangeChoice::KeepLocal => {
                self.set_status(
                    StatusKind::Info,
                    format!("Kept unsaved changes to {}", self.document.display_name()),
                );
                self.rerun_after_external_change(change.rerun_after_resolution);
            }
            ExternalChangeChoice::Reload => self.reload_external_change(change),
            ExternalChangeChoice::SaveCopy => self.save_copy_then_reload(change),
        }
    }

    fn perform(&mut self, intent: Intent) {
        match intent {
            Intent::New => self.set_document(Document::untitled("")),
            Intent::Open => {
                let mut dialog = rfd::FileDialog::new().add_filter("Python script", &["py"]);
                if let Some(dir) = self.document.path().and_then(Path::parent) {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(path) = dialog.pick_file() {
                    self.open_path(&path);
                }
            }
            Intent::OpenPath(path) => self.open_path(&path),
            Intent::OpenExample(index) => {
                if let Some(example) = self.examples.get(index) {
                    let document = Document::example(&example.title, example.source);
                    self.set_document(document);
                    self.set_status(
                        StatusKind::Info,
                        "Examples are read-only; saving asks for a new location",
                    );
                }
            }
            Intent::Quit => {
                self.clear_recovery_session();
                self.allow_close = true;
                self.ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(document) => {
                self.set_document(document);
                self.settings.add_recent_file(path);
                self.settings.last_file = Some(path.to_path_buf());
                self.set_status(StatusKind::Info, format!("Opened {}", path.display()));
            }
            Err(err) => self
                .dialogs
                .error("Cannot open file", format!("{}\n\n{err}", path.display())),
        }
    }

    fn set_document(&mut self, document: Document) {
        self.clear_recovery_session();
        self.document = document;
        self.dialogs.external_change = None;
        self.dialogs.history = None;
        self.next_recovery = Instant::now() + CHECKPOINT_INTERVAL;
        self.editor.reset();
        self.traceback = None;
    }

    /// Returns whether the document ended up saved.
    fn save(&mut self) -> bool {
        self.checkpoint_recovery();
        match self.document.save() {
            Ok(true) => {
                self.after_save();
                true
            }
            Ok(false) => {
                if self.document.example_title().is_some() {
                    self.set_status(
                        StatusKind::Info,
                        "Examples are read-only — choose a new location to save your changes",
                    );
                }
                self.save_as()
            }
            Err(err) => {
                self.dialogs.error("Cannot save", err.to_string());
                false
            }
        }
    }

    fn save_as(&mut self) -> bool {
        self.checkpoint_recovery();
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Python script", &["py"])
            .set_file_name(self.document.path().and_then(Path::file_name).map_or_else(
                || "script.py".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ));
        if let Some(dir) = self.document.path().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        let Some(mut path) = dialog.save_file() else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("py");
        }
        if let Err(error) = self.recovery.archive_path(&path) {
            log::warn!("Cannot archive the previous Save As target: {error}");
        }
        match self.document.save_as(&path) {
            Ok(()) => {
                self.after_save();
                true
            }
            Err(err) => {
                self.dialogs
                    .error("Cannot save", format!("{}\n\n{err}", path.display()));
                false
            }
        }
    }

    fn save_copy_then_reload(&mut self, change: ExternalChange) {
        let stem = change
            .path
            .file_stem()
            .map_or_else(|| "script".into(), |name| name.to_string_lossy());
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Python script", &["py"])
            .set_file_name(format!("{stem}.local-copy.py"));
        if let Some(directory) = change.path.parent() {
            dialog = dialog.set_directory(directory);
        }
        let Some(mut path) = dialog.save_file() else {
            self.dialogs.external_change = Some(change);
            return;
        };
        if path.extension().is_none() {
            path.set_extension("py");
        }
        match self.document.save_copy(&path) {
            Ok(()) => self.reload_external_change(change),
            Err(error) => {
                self.dialogs.error(
                    "Cannot save a copy",
                    format!("{}\n\n{error}", path.display()),
                );
                self.dialogs.external_change = Some(change);
            }
        }
    }

    fn reload_external_change(&mut self, change: ExternalChange) {
        self.checkpoint_recovery();
        let backup = if self.document.is_modified() {
            match self
                .document
                .create_recovery_backup(&self.cache_dir.join("recovery"))
            {
                Ok(path) => Some(path),
                Err(error) => {
                    self.dialogs.error(
                        "Cannot create recovery backup",
                        format!(
                            "The file was not reloaded and your edits are unchanged.\n\n{error}"
                        ),
                    );
                    self.dialogs.external_change = Some(change);
                    return;
                }
            }
        } else {
            None
        };

        match self.document.apply_disk_version(change.disk) {
            Ok(_) => {
                if let Some(path) = &backup {
                    logging::push(
                        Level::Info,
                        "recovery",
                        &format!("Saved unsaved edits to {}", path.display()),
                    );
                }
                let message = backup.map_or_else(
                    || format!("Reloaded {}", self.document.display_name()),
                    |path| {
                        format!(
                            "Reloaded {}; recovery backup: {}",
                            self.document.display_name(),
                            path.display()
                        )
                    },
                );
                self.set_status(StatusKind::Info, message);
                self.clear_recovery_session();
                self.rerun_after_external_change(change.rerun_after_resolution);
            }
            Err(error) => self
                .dialogs
                .error("Cannot reload the script", error.to_string()),
        }
    }

    fn rerun_after_external_change(&mut self, rerun: bool) {
        if rerun {
            self.start_run(false);
        }
    }

    fn after_save(&mut self) {
        if let Err(error) = self.recovery.archive(&self.document) {
            log::warn!("Cannot add the saved document to local history: {error}");
        }
        self.clear_recovery_session();
        if let Some(path) = self.document.path().map(Path::to_path_buf) {
            self.settings.add_recent_file(&path);
            self.settings.last_file = Some(path.clone());
            self.set_status(StatusKind::Success, format!("Saved {}", path.display()));
        }
    }

    fn show_local_history(&mut self) {
        self.checkpoint_recovery();
        match self.recovery.history(self.document.path()) {
            Ok(versions) if versions.is_empty() => self.dialogs.info(
                "Local history",
                "No recovery versions are available for this document yet.",
            ),
            Ok(versions) => {
                self.dialogs.history = Some(HistoryDialog {
                    versions,
                    selected: 0,
                });
            }
            Err(error) => self
                .dialogs
                .error("Cannot read local history", error.to_string()),
        }
    }

    fn checkpoint_recovery(&mut self) {
        // The startup document is a separate clean buffer while the recovered
        // session awaits a decision. It must never clear that pending session.
        if self.dialogs.recovery_session.is_some() {
            return;
        }
        match self.recovery.checkpoint(&self.document) {
            Ok(_) => self.recovery_error_reported = false,
            Err(error) => {
                log::warn!("Cannot create recovery checkpoint: {error}");
                if !self.recovery_error_reported {
                    self.set_status(
                        StatusKind::Warning,
                        "Automatic recovery is unavailable; see the Log panel",
                    );
                    self.recovery_error_reported = true;
                }
            }
        }
    }

    fn clear_recovery_session(&mut self) {
        if let Err(error) = self.recovery.clear_session() {
            log::warn!("Cannot clear recovery session: {error}");
        }
    }

    fn poll_recovery(&mut self, ctx: &egui::Context) {
        if self.dialogs.recovery_session.is_some() {
            return;
        }
        if !self.document.is_modified() {
            self.clear_recovery_session();
            self.next_recovery = Instant::now() + CHECKPOINT_INTERVAL;
            return;
        }

        let now = Instant::now();
        if now >= self.next_recovery {
            self.checkpoint_recovery();
            self.next_recovery = now + CHECKPOINT_INTERVAL;
        }
        ctx.request_repaint_after(self.next_recovery.saturating_duration_since(now));
    }

    // -- worker -------------------------------------------------------------------

    fn start_worker(&mut self) {
        let explicit = self
            .python_override
            .clone()
            .unwrap_or_else(|| self.settings.general.python.clone());
        let python = launch::find_python(Some(&explicit));
        let worker_dir = match embedded::worker_dir(&self.cache_dir) {
            Ok(dir) => dir,
            Err(err) => {
                self.set_status(
                    StatusKind::Error,
                    format!("Cannot prepare the Python worker: {err}"),
                );
                return;
            }
        };
        self.worker.start(LaunchConfig { python, worker_dir });
        self.worker_was_ready = false;
        self.reset_execution_state();
        // The new process knows nothing about the old one's inspections.
        self.inspector.clear();
        self.scene.clear_topology_selection();
        self.measurements.clear();
        self.scene.overlay = Overlay::default();
    }

    /// Forgets everything that was in flight in the worker.
    fn reset_execution_state(&mut self) {
        self.phase = Phase::Idle;
        self.rerun = false;
        self.pending.clear();
        self.console.busy = false;
        self.editor.set_debug_line(None);
        self.inspector.loading = false;
        if let Some(upload) = &mut self.dialogs.upload {
            upload.busy = false;
        }
    }

    /// Sends a request with a fresh id, remembering what it was for.
    fn send_tracked(&mut self, kind: Pending, build: impl FnOnce(RequestId) -> Request) -> bool {
        if !self.worker.is_ready() {
            self.set_status(StatusKind::Warning, "Python is not ready yet");
            return false;
        }
        let id = self.worker.next_id();
        if self.worker.send(&build(id)) {
            self.pending.insert(id, kind);
            true
        } else {
            false
        }
    }

    fn release(&mut self, keys: Vec<NodeKey>) {
        if !keys.is_empty() {
            self.worker.send(&Request::Release { keys });
        }
    }

    fn process_worker_events(&mut self) {
        for event in self.worker.poll() {
            match event {
                Event::Message(message) => self.handle_message(message),
                Event::Stderr(line) => logging::push(Level::Warn, "python", &line),
                Event::Exited => {
                    let reason = match self.worker.status() {
                        worker::Status::Failed(reason) => reason.clone(),
                        _ => "The Python worker stopped".to_owned(),
                    };
                    log::error!("{reason}");
                    let was_busy = self.is_busy();
                    self.reset_execution_state();
                    if self.worker_was_ready {
                        // A crash inside OpenCascade must not leave the
                        // editor without Python: start a fresh worker.
                        self.set_status(
                            StatusKind::Error,
                            if was_busy {
                                "Python crashed while running the script; it was restarted"
                            } else {
                                "Python stopped unexpectedly; it was restarted"
                            },
                        );
                        self.start_worker();
                    } else {
                        self.set_status(StatusKind::Error, reason);
                    }
                }
            }
        }
        // References in a response are only valid while its request runs.
        let pending = &self.pending;
        self.scene.release_geometry(|id| pending.contains_key(&id));
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::Ready(info) => {
                log::info!(
                    "Python {} with CadQuery {} is ready ({})",
                    info.python,
                    info.cadquery,
                    info.executable
                );
                if !info.completion {
                    log::info!("Install jedi in the Python environment for code completion");
                }
                self.worker_was_ready = true;
                self.set_status(
                    StatusKind::Success,
                    format!("CadQuery {} ready", info.cadquery),
                );
                if let Some(debug) = self.run_when_ready.take() {
                    self.start_run(debug);
                }
            }
            Message::Fatal { message, detail } => {
                log::error!("{message}");
                if let Some(detail) = detail {
                    logging::push(Level::Error, "python", &detail);
                }
                self.set_status(StatusKind::Error, message);
            }
            Message::Output { id, stream, text } => {
                let to_console =
                    id.is_some_and(|id| matches!(self.pending.get(&id), Some(Pending::Console)));
                match (to_console, stream) {
                    (true, OutputStream::Stdout) => self.console.push(EntryKind::Output, &text),
                    (true, OutputStream::Stderr) => self.console.push(EntryKind::Error, &text),
                    (false, OutputStream::Stdout) => logging::push(Level::Info, "print", &text),
                    (false, OutputStream::Stderr) => logging::push(Level::Warn, "stderr", &text),
                }
            }
            Message::Log {
                level,
                source,
                message,
            } => {
                logging::push(Level::from_python(&level), &source, &message);
            }
            Message::RunFinished(result) => self.finish_run(result),
            Message::DebugPaused {
                id,
                line,
                variables,
                objects,
            } => {
                self.phase = Phase::Paused { id };
                self.editor.set_debug_line(Some(line));
                self.variables = variables;
                let Some(nodes) = self.decode_scene_nodes(Some(id), objects) else {
                    return;
                };
                let removed = self.scene.replace(nodes, None);
                self.release(removed);
                self.measurements.clear();
                self.refresh_inspector();
                self.set_status(StatusKind::Info, format!("Paused at line {line}"));
            }
            Message::Completions {
                id,
                available,
                items,
                signatures,
            } => {
                if let Some(Pending::ConsoleCompletion) = self.pending.remove(&id) {
                    let inserts: Vec<(String, String)> =
                        items.into_iter().map(|i| (i.name, i.insert)).collect();
                    self.console.complete(&inserts, &signatures);
                } else if !available {
                    self.set_status(
                        StatusKind::Info,
                        "Code completion needs jedi: pip install jedi",
                    );
                } else if !self.editor.show_completions(items, signatures) {
                    self.set_status(StatusKind::Info, "No completions available");
                }
            }
            Message::Inspection { id, key, frames } => {
                self.pending.remove(&id);
                if self.inspector.key.as_deref() == Some(key.as_str()) {
                    self.inspector.frames = frames;
                    self.inspector.loading = false;
                }
            }
            Message::InspectionGeometry { id, meshes, plane } => {
                self.pending.remove(&id);
                if self.inspector.show_in_view {
                    self.scene.overlay = Overlay {
                        meshes: meshes
                            .into_iter()
                            .map(|m| Arc::new(Mesh::from(m)))
                            .collect(),
                        grid: plane.map(|p| Grid {
                            origin: p.origin.into(),
                            x_dir: p.x_dir.into(),
                            z_dir: p.z_dir.into(),
                            size: p.size,
                        }),
                    };
                }
            }
            Message::Measurement(result) => {
                let id = result.id;
                let tracked = matches!(self.pending.remove(&id), Some(Pending::Measure));
                if tracked && self.measurements.request_id == Some(id) {
                    self.measurements.loading = false;
                    self.measurements.error = None;
                    self.measurements.request_id = None;
                    self.measurements.result = Some(result);
                }
            }
            Message::SceneAdd { id, objects } => {
                let was_empty = self.scene.is_empty();
                let Some(nodes) = self.decode_scene_nodes(id, objects) else {
                    return;
                };
                self.scene.append(nodes, None);
                if was_empty {
                    self.viewer.fit(&self.scene, false, self.now());
                }
            }
            Message::ConsoleResult { id, more } => {
                self.pending.remove(&id);
                self.console.more = more;
                self.console.busy = false;
            }
            Message::ExportFinished { id, path } => {
                self.pending.remove(&id);
                self.set_status(StatusKind::Success, format!("Exported {path}"));
            }
            Message::UploadFinished { id, status, body } => {
                self.pending.remove(&id);
                if let Some(upload) = &mut self.dialogs.upload {
                    upload.finished(status, &body);
                }
            }
            Message::Updates { id, current, newer } => {
                self.pending.remove(&id);
                self.status = None;
                if newer.is_empty() {
                    self.dialogs.info(
                        "No updates available",
                        format!(
                            "You are already using the latest version of CadQuery ({current})."
                        ),
                    );
                } else {
                    self.dialogs.info(
                        "Updates available",
                        format!(
                            "There are newer versions of CadQuery available on GitHub:\n\n{}",
                            newer.join("\n")
                        ),
                    );
                }
            }
            Message::Imports { id, paths } => {
                self.pending.remove(&id);
                let mut files: Vec<PathBuf> = self.watcher.watched().cloned().collect();
                files.extend(paths.into_iter().map(PathBuf::from));
                files.dedup();
                self.watcher.set_files(files);
            }
            Message::Error { id, message } => self.request_failed(id, message),
        }
    }

    fn request_failed(&mut self, id: Option<RequestId>, message: String) {
        let request_id = id;
        let kind = id.and_then(|id| self.pending.remove(&id));
        match kind {
            Some(Pending::Console) => {
                self.console.push(EntryKind::Error, &format!("{message}\n"));
                self.console.busy = false;
            }
            Some(Pending::Export) => self.dialogs.error("Export failed", message),
            Some(Pending::Upload) => {
                if let Some(upload) = &mut self.dialogs.upload {
                    upload.failed(&message);
                }
            }
            Some(Pending::Updates) => {
                self.status = None;
                self.dialogs.error("Cannot check for updates", message);
            }
            Some(Pending::Inspect | Pending::InspectGeometry) => {
                self.inspector.loading = false;
                self.set_status(StatusKind::Warning, message);
            }
            Some(Pending::Run) => {
                self.phase = Phase::Idle;
                self.set_status(StatusKind::Error, message);
            }
            Some(Pending::Measure) if self.measurements.request_id == request_id => {
                self.measurements.loading = false;
                self.measurements.error = Some(message);
                self.measurements.request_id = None;
            }
            Some(Pending::Measure) => {}
            Some(Pending::Completion | Pending::ConsoleCompletion | Pending::Imports) | None => {
                log::warn!("{message}");
                self.set_status(StatusKind::Warning, message);
            }
        }
    }

    // -- running scripts ------------------------------------------------------

    fn start_run(&mut self, debug: bool) {
        if !self.worker.is_ready() {
            if matches!(self.worker.status(), worker::Status::Starting) {
                self.run_when_ready = Some(debug);
                self.set_status(StatusKind::Info, "Waiting for Python to start…");
            } else {
                self.set_status(
                    StatusKind::Error,
                    "Python is not available — check Preferences > General",
                );
            }
            return;
        }
        match self.phase {
            Phase::Idle => {}
            Phase::Running { debug: false, .. } if !debug => {
                self.rerun = true;
                return;
            }
            _ => {
                self.set_status(StatusKind::Warning, "Already running; stop it first");
                return;
            }
        }

        // A reload may have changed the text since the editor last drew.
        self.editor.sync_markers(&self.document.text);
        let id = self.worker.next_id();
        // Pinned, so a debugging session may reference them even after it
        // clears the view below.
        let known_geometry = self.scene.known_geometry();
        self.scene.pin_geometry(id, &known_geometry);
        let request = Request::Run {
            id,
            script: self.document.text.clone(),
            path: self.document.path().map(|p| p.display().to_string()),
            debug,
            breakpoints: self.editor.breakpoints.iter().copied().collect(),
            settings: self.settings.run_settings(),
            scene: self.settings.scene_settings(),
            known_geometry,
        };
        if !self.worker.send(&request) {
            self.set_status(StatusKind::Error, "Cannot reach Python");
            return;
        }
        self.pending.insert(id, Pending::Run);
        self.phase = Phase::Running { id, debug };
        if debug {
            // Like the original: the view starts empty and follows the
            // locals of each paused frame.
            let removed = self.scene.clear();
            self.release(removed);
            self.measurements.clear();
            self.set_status(
                StatusKind::Info,
                "Debugging — running to the first breakpoint",
            );
        }
    }

    fn finish_run(&mut self, result: RunFinished) {
        self.pending.remove(&result.id);
        self.phase = Phase::Idle;
        self.editor.set_debug_line(None);

        match &result.error {
            Some(error) => {
                self.editor.error_line = error.line;
                let location = error
                    .line
                    .map(|l| format!(" (line {l})"))
                    .unwrap_or_default();
                self.set_status(
                    StatusKind::Error,
                    format!("{}: {}{location}", error.type_name, error.message),
                );
                logging::push(Level::Error, "script", &error.text);
            }
            None => self.editor.error_line = None,
        }
        let ok = result.ok;
        self.traceback = result.error;

        if let Some(objects) = result.objects {
            let preserved = self
                .settings
                .objects
                .preserve_properties
                .then(|| self.scene.snapshot_properties());
            let was_empty = self.scene.is_empty();
            let Some(nodes) = self.decode_scene_nodes(Some(result.id), objects) else {
                return;
            };
            let count = nodes.len();
            if self.settings.objects.clear_before_run || result.debug {
                let removed = self.scene.replace(nodes, preserved.as_ref());
                self.release(removed);
                self.measurements.clear();
            } else {
                self.scene.append(nodes, preserved.as_ref());
            }
            self.scene.overlay = Overlay::default();
            self.refresh_inspector();
            if self.settings.viewer.fit_automatically || was_empty {
                let animate = self.settings.viewer.animate && !was_empty;
                self.viewer.fit(&self.scene, animate, self.now());
            }
            if ok {
                let plural = if count == 1 { "" } else { "s" };
                self.set_status(
                    StatusKind::Success,
                    format!(
                        "Rendered {count} object{plural} in {:.2} s",
                        result.duration
                    ),
                );
            }
        }
        if let Some(variables) = result.variables {
            self.variables = variables;
        }
        if result.stopped {
            self.set_status(StatusKind::Info, "Debugging stopped");
        }
        if std::mem::take(&mut self.rerun) {
            self.start_run(false);
        }
    }

    fn debug_command(&mut self, action: DebugAction) {
        let Phase::Paused { id } = self.phase else {
            return;
        };
        self.editor.sync_markers(&self.document.text);
        let breakpoints = self.editor.breakpoints.iter().copied().collect();
        self.worker.send(&Request::Debug {
            action,
            breakpoints,
        });
        self.phase = Phase::Running { id, debug: true };
        self.editor.set_debug_line(None);
    }

    fn stop_debugging(&mut self) {
        if self.is_paused() {
            self.debug_command(DebugAction::Stop);
        } else {
            self.stop();
        }
    }

    /// Aborts a running script by restarting the worker.
    fn stop(&mut self) {
        if !self.is_busy() {
            return;
        }
        self.start_worker();
        self.set_status(StatusKind::Warning, "Execution stopped");
    }

    // -- editor services ----------------------------------------------------------

    fn request_completion(&mut self, request: CompletionRequest) {
        if !self.worker.is_ready() {
            return;
        }
        let source = self.document.text.clone();
        let path = self.document.path().map(|p| p.display().to_string());
        self.send_tracked(Pending::Completion, |id| Request::Complete {
            id,
            source,
            line: request.line,
            column: request.column,
            path,
            target: CompletionTarget::Editor,
        });
    }

    fn sync_watcher(&mut self) {
        let key = (
            self.settings.editor.autoreload,
            self.settings.editor.watch_imported_modules,
            self.document.path().map(Path::to_path_buf),
        );
        if self.watch_key.as_ref() == Some(&key) {
            return;
        }
        let (autoreload, watch_imports, path) = key.clone();
        self.watch_key = Some(key);
        match path.filter(|_| autoreload) {
            Some(path) => {
                self.watcher.set_files(vec![path.clone()]);
                if watch_imports {
                    let path = path.display().to_string();
                    self.send_tracked(Pending::Imports, |id| Request::Imports { id, path });
                }
            }
            None => self.watcher.clear(),
        }
    }

    fn poll_watcher(&mut self, ctx: &egui::Context) {
        if !self.watcher.is_watching() {
            return;
        }
        ctx.request_repaint_after(self.watcher.next_poll_in());
        let delay = Duration::from_millis(self.settings.editor.autoreload_delay_ms);
        let changed = self.watcher.poll(delay);
        if changed.is_empty() {
            return;
        }

        let document_path = self.document.path().map(Path::to_path_buf);
        let document_changed = document_path
            .as_ref()
            .is_some_and(|path| changed.iter().any(|changed| changed == path));
        let imported_changed = changed
            .iter()
            .any(|changed| Some(changed) != document_path.as_ref());

        if !document_changed {
            if let Some(conflict) = &mut self.dialogs.external_change {
                conflict.rerun_after_resolution |= imported_changed;
            } else if imported_changed {
                self.start_run(false);
            }
            return;
        }

        let version = match self.document.disk_version() {
            Ok(Some(version)) => version,
            Ok(None) => return,
            Err(error) => {
                log::warn!("Cannot read the changed script: {error}");
                self.dialogs
                    .error("Cannot read the changed script", error.to_string());
                return;
            }
        };

        match self.document.classify_external_update(&version) {
            // A timestamp-only rewrite of the main file must neither erase
            // local edits nor trigger a needless rebuild.
            ExternalUpdate::Unchanged => {
                if imported_changed {
                    self.start_run(false);
                }
            }
            ExternalUpdate::Conflict => {
                self.dialogs.external_change = Some(ExternalChange {
                    path: document_path.expect("a watched document has a path"),
                    disk: version,
                    show_comparison: false,
                    rerun_after_resolution: true,
                });
                self.set_status(
                    StatusKind::Warning,
                    "File changed on disk; waiting for your decision",
                );
            }
            ExternalUpdate::MatchesBuffer => match self.document.apply_disk_version(version) {
                Ok(_) => {
                    self.set_status(
                        StatusKind::Info,
                        format!("Reloaded {}", self.document.display_name()),
                    );
                    self.clear_recovery_session();
                    self.start_run(false);
                }
                Err(error) => log::warn!("Cannot reload the script: {error}"),
            },
            ExternalUpdate::Reload => match self.document.reload(version) {
                Ok(_) => {
                    self.set_status(
                        StatusKind::Info,
                        format!("Reloaded {}", self.document.display_name()),
                    );
                    self.start_run(false);
                }
                Err(error) => log::warn!("Cannot reload the script: {error}"),
            },
        }
    }

    // -- objects and inspection -------------------------------------------------

    fn select(
        &mut self,
        key: Option<NodeKey>,
        topology: Option<crate::scene::TopologySelection>,
        extend: bool,
    ) {
        match (key, topology) {
            (Some(key), Some(selection)) => {
                if extend {
                    self.scene.ensure_selected(&key);
                } else {
                    self.scene.select_only(&key);
                }
                let entity = selection.entity;
                let node = selection.node.clone();
                if self.scene.select_topology(selection, extend) {
                    let count = self.scene.topology_selections.len();
                    if count == 0 {
                        self.set_status(StatusKind::Info, "Measurement selection cleared");
                    } else {
                        let name = self
                            .scene
                            .find(&node)
                            .map_or(node.as_str(), |node| node.name.as_str());
                        self.set_status(
                            StatusKind::Info,
                            format!(
                                "Selected {} {} of {name} · {count}/2 measurement entities",
                                entity.kind(),
                                entity.identifier()
                            ),
                        );
                    }
                }
            }
            (Some(key), None) if extend => self.scene.toggle_selected(&key),
            (Some(key), None) => self.scene.select_only(&key),
            (None, _) if !extend => self.scene.clear_selection(),
            (None, _) => {}
        }
        self.request_measurement();
        self.refresh_inspector();
    }

    fn request_measurement(&mut self) {
        if self.scene.topology_selections.is_empty() {
            self.measurements.clear();
            return;
        }
        if !self.worker.is_ready() {
            self.measurements.clear();
            return;
        }

        let selections = self
            .scene
            .topology_selections
            .iter()
            .map(|selection| TopologyReference {
                key: selection.node.clone(),
                part: selection.part,
                kind: match selection.entity {
                    crate::scene::TopologyEntity::Face(_) => TopologyKind::Face,
                    crate::scene::TopologyEntity::Edge(_) => TopologyKind::Edge,
                    crate::scene::TopologyEntity::Vertex(_) => TopologyKind::Vertex,
                },
                identifier: selection.entity.identifier(),
            })
            .collect();
        let id = self.worker.next_id();
        if self.worker.send(&Request::Measure { id, selections }) {
            self.pending.insert(id, Pending::Measure);
            self.measurements.loading = true;
            self.measurements.error = None;
            self.measurements.request_id = Some(id);
        } else {
            self.measurements.loading = false;
            self.measurements.error = Some("Cannot reach Python".to_owned());
            self.measurements.request_id = None;
        }
    }

    /// Inspects the current object when it is a workplane.
    fn refresh_inspector(&mut self) {
        let target = self
            .scene
            .current()
            .filter(|node| node.inspectable)
            .map(|node| (node.key.clone(), node.name.clone()));
        match target {
            Some((key, name)) => {
                if self.inspector.key.as_deref() == Some(key.as_str()) {
                    return;
                }
                self.inspector.clear();
                self.scene.overlay = Overlay::default();
                self.inspector.key = Some(key.clone());
                self.inspector.name = name;
                self.inspector.loading =
                    self.send_tracked(Pending::Inspect, |id| Request::Inspect { id, key });
            }
            None => {
                let gone = self
                    .inspector
                    .key
                    .as_ref()
                    .is_some_and(|key| self.scene.find(key).is_none());
                if gone {
                    self.inspector.clear();
                    self.scene.overlay = Overlay::default();
                }
            }
        }
    }

    fn toggle_inspect(&mut self) {
        self.inspector.show_in_view = !self.inspector.show_in_view;
        if self.inspector.show_in_view {
            self.request_inspection_geometry();
        } else {
            self.scene.overlay = Overlay::default();
        }
    }

    fn request_inspection_geometry(&mut self) {
        let (Some(key), Some((frame, item))) =
            (self.inspector.key.clone(), self.inspector.selected)
        else {
            return;
        };
        let scene = self.settings.scene_settings();
        self.send_tracked(Pending::InspectGeometry, |id| Request::InspectGeometry {
            id,
            key,
            frame,
            item,
            scene,
        });
    }

    /// The selection, or everything when nothing is selected.
    fn objects_to_process(&self) -> Vec<NodeKey> {
        let selected = self.scene.selected_roots();
        if selected.is_empty() {
            self.scene.top_level_keys()
        } else {
            selected
        }
    }

    fn export(&mut self, format: ExportFormat) {
        let keys = self.objects_to_process();
        if keys.is_empty() {
            self.set_status(StatusKind::Warning, "Nothing to export");
            return;
        }
        let stem = self
            .document
            .path()
            .and_then(Path::file_stem)
            .map_or_else(|| "model".to_owned(), |s| s.to_string_lossy().into_owned());
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter(format.label(), &[format.extension()])
            .set_file_name(format!("{stem}.{}", format.extension()))
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension(format.extension());
        }
        let (tolerance, angular_tolerance) = (
            self.settings.objects.stl_tolerance,
            self.settings.objects.stl_angular_tolerance,
        );
        let path_text = path.display().to_string();
        if self.send_tracked(Pending::Export, |id| Request::Export {
            id,
            keys,
            format,
            path: path_text,
            tolerance,
            angular_tolerance,
        }) {
            self.set_status(StatusKind::Info, format!("Exporting {}…", path.display()));
        }
    }

    fn screenshot(&mut self) {
        if let Some(mut path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("screenshot.png")
            .save_file()
        {
            if path.extension().is_none() {
                path.set_extension("png");
            }
            self.viewer.request_screenshot(path);
        }
    }

    fn console_submit(&mut self, source: String) {
        if !self.worker.is_ready() {
            self.console
                .push(EntryKind::Error, "Python is not running\n");
            return;
        }
        self.console.record_input(&source);
        let scene = self.settings.scene_settings();
        let known_geometry = self.scene.known_geometry();
        let mut sent = None;
        if self.send_tracked(Pending::Console, |id| {
            sent = Some(id);
            Request::Console {
                id,
                source,
                scene,
                known_geometry: known_geometry.clone(),
            }
        }) {
            if let Some(id) = sent {
                self.scene.pin_geometry(id, &known_geometry);
            }
        } else {
            self.console.busy = false;
        }
    }

    fn console_complete(&mut self) {
        let source = self.console.input.clone();
        let line = source.split('\n').count();
        let column = source
            .rsplit('\n')
            .next()
            .map_or(0, |last| last.chars().count());
        self.send_tracked(Pending::ConsoleCompletion, |id| Request::Complete {
            id,
            source,
            line,
            column,
            path: None,
            target: CompletionTarget::Console,
        });
    }

    fn start_upload(&mut self, url: String, file_type: String) {
        let keys = self.objects_to_process();
        let source = self.document.text.clone();
        if !self.send_tracked(Pending::Upload, |id| Request::Upload {
            id,
            url,
            file_type,
            keys,
            source,
        }) && let Some(upload) = &mut self.dialogs.upload
        {
            upload.failed("Python is not ready");
        }
    }
    fn decode_scene_nodes(
        &mut self,
        request: Option<RequestId>,
        objects: Vec<NodeMessage>,
    ) -> Option<Vec<SceneNode>> {
        match self.scene.decode_nodes(request, objects) {
            Ok(nodes) => Some(nodes),
            Err(error) => {
                log::error!("Cannot decode worker scene: {error}");
                self.set_status(StatusKind::Error, "Cannot update the 3D scene");
                None
            }
        }
    }
}

fn open_url(url: &str) {
    if let Err(err) = open::that(url) {
        log::warn!("Cannot open {url}: {err}");
    }
}

pub struct CqEditorApp {
    dock: DockState<Tab>,
    state: AppState,
}

impl CqEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: StartupOptions) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctx.set_fonts(theme::fonts());

        let settings: Settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        let dock = cc
            .storage
            .and_then(|storage| eframe::get_value::<DockState<Tab>>(storage, LAYOUT_KEY))
            .filter(|dock| dock.find_tab(&Tab::Viewer).is_some())
            .unwrap_or_else(panels::default_layout);

        let cache_dir = eframe::storage_dir("cq-editor").unwrap_or_else(std::env::temp_dir);
        let recovery = RecoveryStore::new(cache_dir.join("recovery"));
        let mut startup_errors = Vec::new();
        let recovery_session = match recovery.load_session() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log::warn!("Cannot load the recovery session: {error}");
                startup_errors.push(format!("Cannot load the recovery session:\n{error}"));
                None
            }
        };
        let startup_path = options
            .file
            .clone()
            .or_else(|| settings.last_file.clone().filter(|p| p.is_file()));
        let document = startup_path
            .and_then(|path| match Document::open(&path) {
                Ok(document) => Some(document),
                Err(error) => {
                    log::warn!("Cannot open {}: {error}", path.display());
                    startup_errors.push(format!(
                        "Cannot open the startup file {}:\n{error}",
                        path.display()
                    ));
                    None
                }
            })
            .unwrap_or_else(|| Document::untitled(DEFAULT_SCRIPT));
        let mut dialogs = Dialogs {
            recovery_session,
            ..Dialogs::default()
        };
        if !startup_errors.is_empty() {
            dialogs.error("Startup problem", startup_errors.join("\n\n"));
        }

        let mut state = AppState {
            worker: Worker::new(ctx.clone()),
            ctx,
            settings,
            document,
            editor: CodeEditor::default(),
            scene: Scene::default(),
            viewer: Viewer::default(),
            variables: Vec::new(),
            traceback: None,
            inspector: InspectorState::default(),
            measurements: MeasurementState::default(),
            console: ConsoleState::default(),
            log_view: LogView::default(),
            status: None,
            dialogs,
            phase: Phase::Idle,
            rerun: false,
            run_when_ready: None,
            pending: HashMap::new(),
            actions: Vec::new(),
            watcher: FileWatcher::default(),
            watch_key: None,
            examples: embedded::examples(),
            palette: Palette::DARK,
            cache_dir,
            recovery,
            next_recovery: Instant::now() + CHECKPOINT_INTERVAL,
            recovery_error_reported: false,
            python_override: options.python,
            worker_was_ready: false,
            allow_close: false,
            title: String::new(),
            applied_theme: None,
            applied_scale: 1.0,
            logo: None,
        };
        if let Some(path) = state.document.path().map(Path::to_path_buf) {
            state.settings.add_recent_file(&path);
            state.settings.last_file = Some(path);
        }
        state.start_worker();
        Self { dock, state }
    }

    fn execute(&mut self, action: Action) {
        match action {
            Action::TogglePanel(tab) => match self.dock.find_tab(&tab) {
                Some(path) => {
                    self.dock.remove_tab(path);
                }
                None => self.dock.push_to_focused_leaf(tab),
            },
            Action::ResetLayout => self.dock = panels::default_layout(),
            other => self.state.execute(other),
        }
    }
}

impl eframe::App for CqEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.state.begin_frame(&ctx);
        let palette = self.state.palette;

        let open_panels: Vec<Tab> = self.dock.iter_all_tabs().map(|(_, tab)| *tab).collect();
        let bar = |margin_y: i8| {
            egui::Frame::NONE
                .fill(palette.window)
                .inner_margin(egui::Margin::symmetric(8, margin_y))
        };
        egui::Panel::top("menu_bar").frame(bar(2)).show(ui, |ui| {
            crate::ui::menu_bar::ui(ui, &mut self.state, &open_panels)
        });
        egui::Panel::top("toolbar")
            .frame(bar(3))
            .show(ui, |ui| crate::ui::toolbar::ui(ui, &mut self.state));
        egui::Panel::bottom("status_bar")
            .frame(bar(3))
            .show(ui, |ui| crate::ui::status_bar::ui(ui, &self.state));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(palette.window)
                    .inner_margin(egui::Margin::symmetric(4, 0)),
            )
            .show(ui, |ui| {
                let style = theme::dock_style(ui.style(), &palette);
                DockArea::new(&mut self.dock)
                    .style(style)
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(
                        ui,
                        &mut TabContext {
                            state: &mut self.state,
                            frame,
                        },
                    );
            });

        dialogs::show(&ctx, &mut self.state);
        if let worker::Status::Failed(reason) = self.state.worker.status() {
            let reason = reason.clone();
            let python = self
                .state
                .worker
                .config()
                .map(|c| c.python.display().to_string())
                .unwrap_or_default();
            let mut actions = Vec::new();
            dialogs::python_problem(&ctx, &reason, &python, &mut actions);
            self.state.queue(actions);
        }

        for action in std::mem::take(&mut self.state.actions) {
            log::debug!("action {action:?}");
            self.execute(action);
        }
        self.state.end_frame(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.state.checkpoint_recovery();
        eframe::set_value(storage, SETTINGS_KEY, &self.state.settings);
        eframe::set_value(storage, LAYOUT_KEY, &self.dock);
    }
}
