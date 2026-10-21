//! The dockable panels and their default arrangement.

mod console;
mod inspector;
mod log;
mod objects;
mod traceback;
mod variables;

pub use console::{ConsoleState, EntryKind};
pub use inspector::InspectorState;
pub use log::LogView;

use egui_dock::{DockState, NodeIndex, TabViewer};
use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Editor,
    Viewer,
    Objects,
    Variables,
    Inspector,
    Console,
    Traceback,
    Log,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Editor,
        Tab::Viewer,
        Tab::Objects,
        Tab::Variables,
        Tab::Inspector,
        Tab::Console,
        Tab::Traceback,
        Tab::Log,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Editor => "Editor",
            Tab::Viewer => "3D View",
            Tab::Objects => "Objects",
            Tab::Variables => "Variables",
            Tab::Inspector => "CQ Inspector",
            Tab::Console => "Console",
            Tab::Traceback => "Traceback",
            Tab::Log => "Log",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tab::Editor => icon::CODE,
            Tab::Viewer => icon::CUBE,
            Tab::Objects => icon::TREE_STRUCTURE,
            Tab::Variables => icon::BRACKETS_CURLY,
            Tab::Inspector => icon::STACK,
            Tab::Console => icon::TERMINAL_WINDOW,
            Tab::Traceback => icon::WARNING_CIRCLE,
            Tab::Log => icon::LIST_BULLETS,
        }
    }
}

/// Editor on the left, 3D view in the middle, objects and variables on the
/// right and console, traceback and log at the bottom, like the original.
pub fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Viewer]);
    let tree = dock.main_surface_mut();
    // Fractions give the share of the left (or top) child of each split.
    let [top, bottom] = tree.split_below(NodeIndex::root(), 0.74, vec![Tab::Console]);
    tree.split_right(bottom, 0.45, vec![Tab::Traceback, Tab::Log]);
    let [viewer, _editor] = tree.split_left(top, 0.36, vec![Tab::Editor]);
    let [_viewer, objects] = tree.split_right(viewer, 0.72, vec![Tab::Objects]);
    tree.split_below(objects, 0.6, vec![Tab::Variables, Tab::Inspector]);
    dock
}

/// Shows the panels; holds what they need from the application.
pub struct TabContext<'a> {
    pub state: &'a mut AppState,
    pub frame: &'a eframe::Frame,
}

impl TabViewer for TabContext<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let mut title = format!("{}  {}", tab.icon(), tab.title());
        match tab {
            Tab::Editor => {
                title = format!("{}  {}", tab.icon(), self.state.document.display_name());
                if self.state.document.is_modified() {
                    title.push_str(" •");
                }
            }
            Tab::Traceback if self.state.traceback.is_some() => title.push_str(" •"),
            _ => {}
        }
        title.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Editor => self.state.editor_ui(ui),
            Tab::Viewer => self.state.viewer_ui(ui, self.frame),
            Tab::Objects => objects::ui(ui, self.state),
            Tab::Variables => variables::ui(ui, self.state),
            Tab::Inspector => inspector::ui(ui, self.state),
            Tab::Console => console::ui(ui, self.state),
            Tab::Traceback => traceback::ui(ui, self.state),
            Tab::Log => log::ui(ui, self.state),
        }
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        *tab != Tab::Viewer
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        // Every panel scrolls its own content.
        [false, false]
    }
}

/// Centered, dimmed text for empty panels.
pub(crate) fn empty_state(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.weak(format!("{icon}  {text}"));
    });
}
