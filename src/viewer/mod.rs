//! The interactive 3D view.
//!
//! Mouse controls (as in the original editor):
//! * left drag: orbit (turntable or trackball); left click: select
//! * middle drag or Ctrl + left drag: pan
//! * right drag or Shift + left drag: zoom; mouse wheel: zoom at the pointer
//! * Ctrl + click: add to / remove from the selection

pub mod camera;
mod overlays;
mod picking;
mod renderer;

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use glam::{Vec2 as GVec2, Vec3};

use crate::scene::{Grid, Mesh, NodeKey, Scene, TopologySelection};
use crate::settings::{self, OrbitMethod};
use camera::{Animation, Camera, StandardView};
pub use overlays::CubeColors;
use overlays::{NavAction, NavigationCube};
use renderer::{DrawItem, FrameInput, Layer, SceneRenderer, Style};

/// Zoom factor per point of scrolling (a wheel notch is about 50 points).
const ZOOM_PER_SCROLL_POINT: f32 = 0.9978;
/// Zoom factor per point of vertical drag with the zoom button.
const ZOOM_PER_DRAG_POINT: f32 = 1.006;
/// Duration of camera transitions, seconds.
const ANIMATION_SECONDS: f64 = 0.25;
const POINT_SIZE: f32 = 7.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    #[default]
    Shaded,
    Wireframe,
}

/// Colors the view takes from the UI theme.
#[derive(Clone)]
pub struct ViewerColors {
    pub selection: Color32,
    pub hover: Color32,
    pub overlay: Color32,
    pub grid: Color32,
    pub cube: CubeColors,
    pub overlay_text: Color32,
    pub overlay_background: Color32,
}

/// What happened in the view this frame.
pub enum ViewerEvent {
    /// A click selected `key` (or nothing); `extend` toggles instead of
    /// replacing the selection.
    Picked {
        key: Option<NodeKey>,
        topology: Option<TopologySelection>,
        extend: bool,
    },
    /// The user double-clicked an object.
    FocusRequested,
    Screenshot(Result<PathBuf, String>),
}

/// Per-frame inputs from the application.
pub struct ViewInput<'a> {
    pub scene: &'a Scene,
    pub settings: &'a settings::Viewer,
    pub colors: &'a ViewerColors,
    /// Shown as a status pill while the worker is busy.
    pub busy: Option<&'a str>,
    /// Shown in the middle of an empty view.
    pub empty_hint: Option<&'a str>,
}

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Orbit,
    Pan,
    Zoom,
}

struct Drag {
    mode: DragMode,
    start: Pos2,
}

pub struct Viewer {
    pub camera: Camera,
    pub display_mode: DisplayMode,
    animation: Option<Animation>,
    renderer: Option<SceneRenderer>,
    nav_cube: NavigationCube,
    drag: Option<Drag>,
    hovered: Option<NodeKey>,
    last_pointer: Option<Pos2>,
    orbit_method: OrbitMethod,
    aspect: f32,
    screenshot: Option<PathBuf>,
    axis_meshes: Option<(f32, [Arc<Mesh>; 3])>,
    grid_mesh: Option<(GridKey, Arc<Mesh>)>,
}

#[derive(Clone, Copy, PartialEq)]
struct GridKey([f32; 10]);

impl Default for Viewer {
    fn default() -> Self {
        Self {
            camera: Camera::default(),
            display_mode: DisplayMode::Shaded,
            animation: None,
            renderer: None,
            nav_cube: NavigationCube::default(),
            drag: None,
            hovered: None,
            last_pointer: None,
            orbit_method: OrbitMethod::Turntable,
            aspect: 1.5,
            screenshot: None,
            axis_meshes: None,
            grid_mesh: None,
        }
    }
}

impl Viewer {
    /// Frames everything visible.
    pub fn fit(&mut self, scene: &Scene, animate: bool, now: f64) {
        let pose = self.camera.fit_pose(scene.visible_bounds(), self.aspect);
        self.animate_to(pose, animate, now);
    }

    /// Frames the selection, or everything when nothing is selected.
    pub fn fit_selection(&mut self, scene: &Scene, animate: bool, now: f64) {
        let bounds = scene.selection_bounds();
        if bounds.is_empty() {
            self.fit(scene, animate, now);
        } else {
            let pose = self.camera.fit_pose(bounds, self.aspect);
            self.animate_to(pose, animate, now);
        }
    }

    pub fn set_standard_view(&mut self, view: StandardView, animate: bool, now: f64) {
        let mut pose = self.settled_pose();
        pose.rotation = view.rotation();
        self.animate_to(pose, animate, now);
    }

    pub fn request_screenshot(&mut self, path: PathBuf) {
        self.screenshot = Some(path);
    }

    /// The pose the camera is heading to (the current one when idle).
    fn settled_pose(&self) -> camera::Pose {
        self.animation
            .as_ref()
            .map_or_else(|| self.camera.pose(), Animation::target)
    }

    fn animate_to(&mut self, pose: camera::Pose, animate: bool, now: f64) {
        if animate {
            self.animation = Some(Animation::new(
                self.camera.pose(),
                pose,
                now,
                ANIMATION_SECONDS,
            ));
        } else {
            self.animation = None;
            self.camera.set_pose(pose);
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        frame: &eframe::Frame,
        input: ViewInput<'_>,
    ) -> Vec<ViewerEvent> {
        let mut events = Vec::new();
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let now = ui.input(|i| i.time);
        self.aspect = (rect.width() / rect.height().max(1.0)).max(1e-3);

        self.apply_settings(input.settings);
        self.update_scene_extent(input.scene);
        self.handle_input(ui, &response, rect, &input, now, &mut events);

        if let Some(animation) = &self.animation {
            let (pose, done) = animation.sample(now);
            self.camera.set_pose(pose);
            if done {
                self.animation = None;
            } else {
                ui.ctx().request_repaint();
            }
        }

        let painter = ui.painter_at(rect);
        match frame.wgpu_render_state() {
            Some(render_state) => {
                let items = self.draw_items(&input);
                let renderer = self
                    .renderer
                    .get_or_insert_with(|| SceneRenderer::new(render_state));
                let ppp = ui.ctx().pixels_per_point();
                let size = [
                    (rect.width() * ppp).round().max(1.0) as u32,
                    (rect.height() * ppp).round().max(1.0) as u32,
                ];
                let settings = input.settings;
                let top = linear_rgba(settings.background, 1.0);
                let bottom = if settings.use_gradient {
                    linear_rgba(settings.background_aux, 1.0)
                } else {
                    top
                };
                let texture = {
                    let mut egui_renderer = render_state.renderer.write();
                    renderer.render(
                        &mut egui_renderer,
                        &FrameInput {
                            camera: &self.camera,
                            size,
                            background_top: top,
                            background_bottom: bottom,
                            items: &items,
                        },
                    )
                };
                painter.image(
                    texture,
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                if let Some(path) = self.screenshot.take() {
                    events.push(ViewerEvent::Screenshot(save_screenshot(renderer, path)));
                }
            }
            None => {
                painter.rect_filled(rect, 0.0, Color32::from_gray(40));
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "3D view unavailable: no GPU renderer",
                    egui::FontId::proportional(14.0),
                    Color32::LIGHT_GRAY,
                );
            }
        }

        self.paint_overlays(&painter, rect, &input);
        events
    }

    fn apply_settings(&mut self, settings: &settings::Viewer) {
        self.camera.projection = settings.projection;
        if settings.orbit_method != self.orbit_method {
            self.orbit_method = settings.orbit_method;
            if self.orbit_method == OrbitMethod::Turntable {
                self.camera.level();
            }
        }
    }

    /// Keeps the clipping planes around everything that can be drawn.
    fn update_scene_extent(&mut self, scene: &Scene) {
        let bounds = scene.visible_bounds();
        let mut radius = if bounds.is_empty() {
            10.0
        } else {
            bounds.diagonal() * 0.5
        };
        let mut center = if bounds.is_empty() {
            Vec3::ZERO
        } else {
            bounds.center()
        };
        if scene.axes.iter().any(|a| a.visible) {
            // The helpers run through the origin.
            let half = scene.axis_half_length();
            radius = radius.max(half + center.length());
            center *= 0.5;
        }
        if let Some(grid) = &scene.overlay.grid {
            radius = radius.max(grid.size * 1.5 + (grid.origin - center).length());
        }
        self.camera.scene_center = center;
        self.camera.scene_radius = radius.max(1e-3);
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: Rect,
        input: &ViewInput<'_>,
        now: f64,
        events: &mut Vec<ViewerEvent>,
    ) {
        let show_cube = input.settings.show_navigation_cube;
        let pointer = response.hover_pos();
        let over_cube = |p: Pos2| show_cube && NavigationCube::contains(rect, p);

        if show_cube {
            let hover_pos = pointer.filter(|_| self.drag.is_none());
            if self.nav_cube.hover(rect, self.camera.rotation, hover_pos) {
                ui.ctx().request_repaint();
            }
        } else {
            self.nav_cube.clear_hover();
        }

        // Dragging.
        if response.drag_started() {
            let start = ui
                .input(|i| i.pointer.press_origin())
                .or(pointer)
                .unwrap_or(rect.center());
            let (modifiers, primary, middle, secondary) = ui.input(|i| {
                (
                    i.modifiers,
                    i.pointer.button_down(egui::PointerButton::Primary),
                    i.pointer.button_down(egui::PointerButton::Middle),
                    i.pointer.button_down(egui::PointerButton::Secondary),
                )
            });
            let mode = if middle || (primary && modifiers.command) {
                Some(DragMode::Pan)
            } else if secondary || (primary && modifiers.shift) {
                Some(DragMode::Zoom)
            } else if primary && !over_cube(start) {
                Some(DragMode::Orbit)
            } else {
                None
            };
            self.drag = mode.map(|mode| Drag { mode, start });
            if self.drag.is_some() {
                self.settle_animation();
            }
        }
        if let Some(drag) = &self.drag
            && response.dragged()
        {
            let delta = response.drag_delta();
            match drag.mode {
                DragMode::Orbit => match self.orbit_method {
                    OrbitMethod::Turntable => {
                        self.camera.orbit_turntable(GVec2::new(delta.x, delta.y))
                    }
                    OrbitMethod::Trackball => {
                        if let Some(current) = pointer {
                            let previous = current - delta;
                            self.camera.orbit_trackball(
                                trackball_coords(rect, previous),
                                trackball_coords(rect, current),
                            );
                        }
                    }
                },
                DragMode::Pan => self.camera.pan(GVec2::new(delta.x, delta.y), rect.height()),
                DragMode::Zoom => {
                    let anchor = ndc(rect, drag.start);
                    self.camera
                        .zoom(ZOOM_PER_DRAG_POINT.powf(delta.y), anchor, self.aspect);
                }
            }
        }
        if response.drag_stopped() {
            self.drag = None;
        }

        // Wheel and pinch zoom at the pointer.
        if let Some(pos) = pointer.filter(|_| response.hovered()) {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = ZOOM_PER_SCROLL_POINT.powf(scroll) / pinch;
            if (factor - 1.0).abs() > 1e-6 {
                self.settle_animation();
                self.camera.zoom(factor, ndc(rect, pos), self.aspect);
            }
        }

        // Clicks: the cube, then object selection.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if over_cube(pos) {
                if let Some(action) = self.nav_cube.click(rect, self.camera.rotation, pos) {
                    self.apply_nav_action(action, input.settings.animate, now);
                }
            } else {
                let extend = ui.input(|i| i.modifiers.command);
                let picked = self.pick(input.scene, rect, pos);
                events.push(ViewerEvent::Picked {
                    key: picked.as_ref().map(|(key, _)| key.clone()),
                    topology: picked.and_then(|(_, topology)| topology),
                    extend,
                });
            }
        }
        if response.double_clicked() && pointer.is_some_and(|p| !over_cube(p)) {
            events.push(ViewerEvent::FocusRequested);
        }

        // Hover highlighting, recomputed only when the pointer moves.
        let hover_pos = pointer.filter(|p| self.drag.is_none() && !over_cube(*p));
        if hover_pos != self.last_pointer {
            self.last_pointer = hover_pos;
            let hovered = hover_pos
                .and_then(|p| self.pick(input.scene, rect, p))
                .map(|(key, _)| key);
            if hovered != self.hovered {
                self.hovered = hovered;
                ui.ctx().request_repaint();
            }
        }
    }

    fn apply_nav_action(&mut self, action: NavAction, animate: bool, now: f64) {
        let mut pose = self.settled_pose();
        match action {
            NavAction::LookFrom { eye_dir, up } => {
                pose.rotation = camera::look_rotation(eye_dir, up)
            }
            NavAction::Roll(angle) => {
                let forward = pose.rotation * Vec3::NEG_Z;
                pose.rotation =
                    (glam::Quat::from_axis_angle(forward, angle) * pose.rotation).normalize();
            }
        }
        self.settle_animation();
        self.animate_to(pose, animate, now);
    }

    /// Jumps to the end of a running animation, so user input starts from a
    /// stable camera.
    fn settle_animation(&mut self) {
        if let Some(animation) = self.animation.take() {
            self.camera.set_pose(animation.target());
        }
    }

    fn pick(
        &self,
        scene: &Scene,
        rect: Rect,
        pos: Pos2,
    ) -> Option<(NodeKey, Option<TopologySelection>)> {
        let candidates = scene.visible_nodes().into_iter().flat_map(|node| {
            node.parts
                .iter()
                .enumerate()
                .map(move |(part_index, part)| picking::Candidate {
                    id: (node.key.clone(), part_index),
                    mesh: &part.mesh,
                })
        });
        picking::pick(
            &self.camera,
            self.aspect,
            ndc(rect, pos),
            GVec2::new(rect.width(), rect.height()),
            candidates,
        )
        .map(|hit| {
            let (node, part) = hit.id;
            let topology = hit.entity.map(|entity| TopologySelection {
                node: node.clone(),
                part,
                entity,
                position: hit.position,
            });
            (node, topology)
        })
    }

    fn draw_items(&mut self, input: &ViewInput<'_>) -> Vec<DrawItem> {
        let (scene, settings, colors) = (input.scene, input.settings, input.colors);
        let default_color = srgb_unit(settings.default_color);
        let default_edge = linear_rgba(settings.edge_color, 1.0);
        let wireframe = self.display_mode == DisplayMode::Wireframe;
        let mut items = Vec::new();

        // Selecting an assembly highlights all of its parts.
        let mut highlighted = std::collections::HashSet::new();
        for key in &scene.selection {
            if let Some(node) = scene.find(key) {
                collect_keys(node, &mut highlighted);
            }
        }

        let topology_node = scene
            .topology_selection
            .as_ref()
            .map(|selection| selection.node.as_str());
        for node in scene.visible_nodes() {
            let selected =
                highlighted.contains(&node.key) && topology_node != Some(node.key.as_str());
            let hovered = self.hovered.as_deref() == Some(node.key.as_str());
            for part in &node.parts {
                let (rgb, alpha) = node.part_color(part, default_color);
                let has_faces = !part.mesh.indices.is_empty();
                let highlight = if selected {
                    with_alpha(colors.selection, 0.6)
                } else if hovered {
                    with_alpha(colors.hover, 0.25)
                } else {
                    [0.0; 4]
                };
                let edge_color = if selected {
                    linear_color(colors.selection, 1.0)
                } else if !has_faces {
                    // Wires and points have no faces to carry the color.
                    rgb_to_linear(rgb, alpha)
                } else {
                    node.edge_color
                        .map_or(default_edge, |c| rgb_to_linear(c, 1.0))
                };
                items.push(DrawItem {
                    mesh: part.mesh.clone(),
                    style: Style {
                        face_color: rgb_to_linear(rgb, alpha),
                        edge_color,
                        highlight,
                        edge_width: if has_faces {
                            settings.edge_width
                        } else {
                            settings.edge_width + 1.0
                        } + if selected || hovered { 0.5 } else { 0.0 },
                        point_size: POINT_SIZE,
                        faces: !wireframe,
                        edges: settings.show_edges || wireframe || !has_faces,
                        points: true,
                    },
                    layer: Layer::Scene,
                });
            }
        }

        if let Some(mesh) = scene.topology_highlight() {
            let color = linear_color(colors.selection, 1.0);
            items.push(DrawItem {
                mesh: mesh.clone(),
                style: Style {
                    face_color: linear_color(colors.selection, 0.55),
                    edge_color: color,
                    highlight: [0.0; 4],
                    edge_width: settings.edge_width + 2.0,
                    point_size: POINT_SIZE + 5.0,
                    faces: true,
                    edges: true,
                    points: true,
                },
                layer: Layer::Overlay,
            });
        }

        let half_length = scene.axis_half_length();
        if self
            .axis_meshes
            .as_ref()
            .is_none_or(|(len, _)| *len != half_length)
        {
            let line = |dir: Vec3| {
                Arc::new(Mesh::lines(vec![
                    (-dir * half_length).to_array(),
                    (dir * half_length).to_array(),
                ]))
            };
            let [x, y, z] = &scene.axes;
            self.axis_meshes = Some((
                half_length,
                [line(x.direction), line(y.direction), line(z.direction)],
            ));
        }
        if let Some((_, meshes)) = &self.axis_meshes {
            for (axis, mesh) in scene.axes.iter().zip(meshes) {
                if axis.visible {
                    items.push(line_item(
                        mesh.clone(),
                        rgb_to_linear(axis.color, 1.0),
                        1.5,
                        Layer::Scene,
                    ));
                }
            }
        }

        if let Some(grid) = &scene.overlay.grid {
            let key = grid_key(grid);
            if self.grid_mesh.as_ref().is_none_or(|(k, _)| *k != key) {
                self.grid_mesh = Some((key, Arc::new(grid_mesh(grid))));
            }
            if let Some((_, mesh)) = &self.grid_mesh {
                items.push(line_item(
                    mesh.clone(),
                    linear_color(colors.grid, 0.8),
                    1.0,
                    Layer::Scene,
                ));
            }
        }

        for mesh in &scene.overlay.meshes {
            items.push(DrawItem {
                mesh: mesh.clone(),
                style: Style {
                    face_color: linear_color(colors.overlay, 0.55),
                    edge_color: linear_color(colors.overlay, 1.0),
                    highlight: [0.0; 4],
                    edge_width: 3.0,
                    point_size: POINT_SIZE + 3.0,
                    faces: true,
                    edges: true,
                    points: true,
                },
                layer: Layer::Overlay,
            });
        }
        items
    }

    fn paint_overlays(&self, painter: &egui::Painter, rect: Rect, input: &ViewInput<'_>) {
        let colors = input.colors;
        if input.settings.show_navigation_cube {
            self.nav_cube
                .paint(painter, rect, self.camera.rotation, &colors.cube);
        }
        if input.settings.show_axes_indicator {
            overlays::paint_axes_indicator(
                painter,
                rect,
                self.camera.rotation,
                [
                    Color32::from_rgb(230, 60, 60),
                    Color32::from_rgb(90, 200, 60),
                    Color32::from_rgb(70, 120, 240),
                ],
                colors.overlay_text,
            );
        }

        if let Some(text) = input.busy {
            let galley = painter.layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(13.0),
                colors.overlay_text,
            );
            let size = galley.size() + Vec2::new(44.0, 14.0);
            let pill = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 24.0), size);
            painter.rect_filled(pill, size.y / 2.0, colors.overlay_background);
            let spinner_center = Pos2::new(pill.left() + 18.0, pill.center().y);
            paint_spinner(
                painter,
                spinner_center,
                6.0,
                colors.overlay_text,
                painter.ctx().input(|i| i.time),
            );
            painter.galley(
                Pos2::new(pill.left() + 32.0, pill.center().y - galley.size().y / 2.0),
                galley,
                colors.overlay_text,
            );
            painter.ctx().request_repaint();
        } else if let Some(hint) = input.empty_hint.filter(|_| input.scene.is_empty()) {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                hint,
                egui::FontId::proportional(15.0),
                colors.overlay_text.gamma_multiply(0.75),
            );
        }
    }
}

fn collect_keys<'a>(
    node: &'a crate::scene::SceneNode,
    out: &mut std::collections::HashSet<&'a NodeKey>,
) {
    out.insert(&node.key);
    for child in &node.children {
        collect_keys(child, out);
    }
}

fn line_item(mesh: Arc<Mesh>, color: [f32; 4], width: f32, layer: Layer) -> DrawItem {
    DrawItem {
        mesh,
        style: Style {
            face_color: color,
            edge_color: color,
            highlight: [0.0; 4],
            edge_width: width,
            point_size: POINT_SIZE,
            faces: false,
            edges: true,
            points: false,
        },
        layer,
    }
}

fn grid_key(grid: &Grid) -> GridKey {
    GridKey([
        grid.origin.x,
        grid.origin.y,
        grid.origin.z,
        grid.x_dir.x,
        grid.x_dir.y,
        grid.x_dir.z,
        grid.z_dir.x,
        grid.z_dir.y,
        grid.z_dir.z,
        grid.size,
    ])
}

/// A square grid on the plane, `size` wide on each side of the origin,
/// with 20 cells per side as in the original editor.
fn grid_mesh(grid: &Grid) -> Mesh {
    let x = grid.x_dir.normalize_or(Vec3::X);
    let y = grid.z_dir.normalize_or(Vec3::Z).cross(x);
    let size = grid.size.max(1e-3);
    let step = size / 20.0;
    let mut segments = Vec::new();
    for i in -20..=20 {
        let offset = i as f32 * step;
        for (a, b) in [
            (
                grid.origin + x * offset - y * size,
                grid.origin + x * offset + y * size,
            ),
            (
                grid.origin + y * offset - x * size,
                grid.origin + y * offset + x * size,
            ),
        ] {
            segments.push(a.to_array());
            segments.push(b.to_array());
        }
    }
    Mesh::lines(segments)
}

/// Normalized device coordinates of `pos` within `rect` (y up).
fn ndc(rect: Rect, pos: Pos2) -> GVec2 {
    GVec2::new(
        (pos.x - rect.left()) / rect.width() * 2.0 - 1.0,
        1.0 - (pos.y - rect.top()) / rect.height() * 2.0,
    )
}

fn trackball_coords(rect: Rect, pos: Pos2) -> GVec2 {
    let half = rect.width().min(rect.height()) / 2.0;
    let offset = pos - rect.center();
    GVec2::new(offset.x / half, -offset.y / half)
}

fn paint_spinner(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, time: f64) {
    let start = (time * 5.0) as f32;
    let points: Vec<Pos2> = (0..=16)
        .map(|i| {
            let angle = start + i as f32 / 16.0 * std::f32::consts::PI * 1.4;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(2.0, color)));
}

fn save_screenshot(renderer: &SceneRenderer, path: PathBuf) -> Result<PathBuf, String> {
    let ([width, height], pixels) = renderer
        .read_pixels()
        .ok_or_else(|| "the GPU did not return the image".to_owned())?;
    image::save_buffer(
        &path,
        &pixels,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|err| err.to_string())?;
    Ok(path)
}

// -- colors ------------------------------------------------------------------

fn srgb_unit(color: [u8; 3]) -> [f32; 3] {
    color.map(|c| f32::from(c) / 255.0)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn rgb_to_linear(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    [
        srgb_to_linear(rgb[0]),
        srgb_to_linear(rgb[1]),
        srgb_to_linear(rgb[2]),
        alpha,
    ]
}

fn linear_rgba(color: [u8; 3], alpha: f32) -> [f32; 4] {
    rgb_to_linear(srgb_unit(color), alpha)
}

fn linear_color(color: Color32, alpha: f32) -> [f32; 4] {
    linear_rgba([color.r(), color.g(), color.b()], alpha)
}

/// Highlight tint: linear RGB with the mix factor in alpha.
fn with_alpha(color: Color32, amount: f32) -> [f32; 4] {
    let [r, g, b, _] = linear_color(color, 1.0);
    [r, g, b, amount]
}
