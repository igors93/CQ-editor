//! What the viewer shows: the objects of the last run, the axis helpers and
//! the inspector overlay.
//!
//! Nodes mirror the worker's scene nodes (one per shown object, assemblies as
//! subtrees). Display state that belongs to the GUI (visibility, color
//! overrides, selection) lives here too.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::Vec3;

use crate::worker::protocol;

pub type NodeKey = String;

/// sRGB color with components in 0..1.
pub type Rgb = [f32; 3];

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Aabb::EMPTY
    }
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    pub fn union(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn diagonal(&self) -> f32 {
        self.size().length()
    }

    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Aabb {
        points.into_iter().fold(Aabb::EMPTY, |b, p| Aabb {
            min: b.min.min(p),
            max: b.max.max(p),
        })
    }
}

/// Triangulated geometry on the CPU side. The renderer caches GPU buffers by
/// [`Mesh::id`].
#[derive(Debug, Default)]
pub struct Mesh {
    pub id: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Line segments, two points each.
    pub edges: Vec<[f32; 3]>,
    pub points: Vec<[f32; 3]>,
    pub bounds: Aabb,
}

impl Mesh {
    pub fn new(
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
        edges: Vec<[f32; 3]>,
        points: Vec<[f32; 3]>,
    ) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let bounds = Aabb::from_points(
            positions
                .iter()
                .chain(&edges)
                .chain(&points)
                .map(|p| Vec3::from_array(*p)),
        );
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            positions,
            normals,
            indices,
            edges,
            points,
            bounds,
        }
    }

    /// Only edges, e.g. for helper lines and grids.
    pub fn lines(segments: Vec<[f32; 3]>) -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new(), segments, Vec::new())
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

impl From<protocol::Mesh> for Mesh {
    fn from(mesh: protocol::Mesh) -> Self {
        let mut normals = mesh.normals;
        normals.resize(mesh.positions.len(), [0.0, 0.0, 1.0]);
        let [min, max] = mesh.bbox;
        Mesh {
            // The worker already measured the geometry.
            bounds: Aabb {
                min: Vec3::from_array(min),
                max: Vec3::from_array(max),
            },
            ..Mesh::new(
                mesh.positions,
                normals,
                mesh.indices,
                mesh.edges,
                mesh.points,
            )
        }
    }
}

#[derive(Debug)]
pub struct Part {
    pub mesh: Arc<Mesh>,
    pub color: Option<Rgb>,
    pub alpha: Option<f32>,
}

/// Display properties the user can change in the Objects panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    pub color: Option<Rgb>,
    pub alpha: Option<f32>,
}

#[derive(Debug)]
pub struct SceneNode {
    pub key: NodeKey,
    pub name: String,
    pub kind: String,
    /// Workplanes whose construction stack the inspector can show.
    pub inspectable: bool,
    pub parts: Vec<Part>,
    pub children: Vec<SceneNode>,
    pub visible: bool,
    /// From `show_object` options.
    pub color: Option<Rgb>,
    pub alpha: Option<f32>,
    pub edge_color: Option<Rgb>,
    pub overrides: Overrides,
}

impl SceneNode {
    pub fn from_message(message: protocol::SceneNode) -> Self {
        Self {
            key: message.key,
            name: message.name,
            kind: message.kind,
            inspectable: message.inspectable,
            parts: message
                .parts
                .into_iter()
                .map(|part| Part {
                    mesh: Arc::new(part.mesh.into()),
                    color: part.color,
                    alpha: part.alpha,
                })
                .collect(),
            children: message
                .children
                .into_iter()
                .map(Self::from_message)
                .collect(),
            visible: true,
            color: message.color,
            alpha: message.alpha,
            edge_color: message.edge_color,
            overrides: Overrides::default(),
        }
    }

    /// Bounds of this node's own parts (not its children).
    pub fn own_bounds(&self) -> Aabb {
        self.parts
            .iter()
            .fold(Aabb::EMPTY, |b, part| b.union(part.mesh.bounds))
    }

    pub fn subtree_bounds(&self) -> Aabb {
        self.children.iter().fold(self.own_bounds(), |b, child| {
            b.union(child.subtree_bounds())
        })
    }

    pub fn triangle_count(&self) -> usize {
        self.parts
            .iter()
            .map(|p| p.mesh.triangle_count())
            .sum::<usize>()
            + self
                .children
                .iter()
                .map(SceneNode::triangle_count)
                .sum::<usize>()
    }

    /// The color a part is drawn with, without selection highlighting:
    /// user override, then the assembly part color, then `show_object`
    /// options, then `default`.
    pub fn part_color(&self, part: &Part, default: Rgb) -> (Rgb, f32) {
        let color = self
            .overrides
            .color
            .or(part.color)
            .or(self.color)
            .unwrap_or(default);
        let alpha = self
            .overrides
            .alpha
            .or(part.alpha)
            .or(self.alpha)
            .unwrap_or(1.0);
        (color, alpha.clamp(0.0, 1.0))
    }

    /// Color for the tree swatch.
    pub fn display_color(&self, default: Rgb) -> Option<(Rgb, f32)> {
        self.parts
            .first()
            .map(|part| self.part_color(part, default))
            .or_else(|| self.children.first().and_then(|c| c.display_color(default)))
    }

    fn visit<'a>(&'a self, f: &mut impl FnMut(&'a SceneNode)) {
        f(self);
        for child in &self.children {
            child.visit(f);
        }
    }

    fn visit_mut(&mut self, f: &mut impl FnMut(&mut SceneNode)) {
        f(self);
        for child in &mut self.children {
            child.visit_mut(f);
        }
    }
}

/// One of the X/Y/Z axis lines through the origin.
#[derive(Clone, Debug)]
pub struct AxisHelper {
    pub name: &'static str,
    pub direction: Vec3,
    pub color: Rgb,
    pub visible: bool,
}

/// Construction plane shown by the inspector.
#[derive(Clone, Debug)]
pub struct Grid {
    pub origin: Vec3,
    pub x_dir: Vec3,
    pub z_dir: Vec3,
    pub size: f32,
}

/// Geometry highlighted by the CQ object inspector.
#[derive(Default)]
pub struct Overlay {
    pub meshes: Vec<Arc<Mesh>>,
    pub grid: Option<Grid>,
}

/// Default half length of the axis helpers when nothing is shown.
const DEFAULT_AXIS_HALF_LENGTH: f32 = 100.0;

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub axes: [AxisHelper; 3],
    /// Selected node keys; the last one is the "current" object.
    pub selection: Vec<NodeKey>,
    pub overlay: Overlay,
}

impl Default for Scene {
    fn default() -> Self {
        let axis = |name, direction, color| AxisHelper {
            name,
            direction,
            color,
            visible: true,
        };
        Self {
            nodes: Vec::new(),
            axes: [
                axis("X", Vec3::X, [1.0, 0.0, 0.0]),
                // "lawngreen", as in the original editor.
                axis("Y", Vec3::Y, [124.0 / 255.0, 252.0 / 255.0, 0.0]),
                axis("Z", Vec3::Z, [0.0, 0.0, 1.0]),
            ],
            selection: Vec::new(),
            overlay: Overlay::default(),
        }
    }
}

/// Visibility and overrides of nodes, keyed by their name path, so they can
/// be carried over to the next run.
pub type PreservedProperties = HashMap<String, (bool, Overrides)>;

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Replaces all objects; returns the keys of the removed top-level nodes.
    pub fn replace(
        &mut self,
        nodes: Vec<SceneNode>,
        preserved: Option<&PreservedProperties>,
    ) -> Vec<NodeKey> {
        let removed = self.clear();
        self.append(nodes, preserved);
        removed
    }

    pub fn append(&mut self, mut nodes: Vec<SceneNode>, preserved: Option<&PreservedProperties>) {
        if let Some(preserved) = preserved {
            for node in &mut nodes {
                restore(node, "", preserved);
            }
        }
        self.nodes.extend(nodes);
    }

    /// Removes all objects; returns the removed top-level keys.
    pub fn clear(&mut self) -> Vec<NodeKey> {
        self.selection.clear();
        self.nodes.drain(..).map(|n| n.key).collect()
    }

    /// Removes the given top-level nodes (keys of nested nodes are ignored).
    pub fn remove_top_level(&mut self, keys: &[NodeKey]) -> Vec<NodeKey> {
        let mut removed = Vec::new();
        self.nodes.retain(|node| {
            let remove = keys.contains(&node.key);
            if remove {
                removed.push(node.key.clone());
            }
            !remove
        });
        self.prune_selection();
        removed
    }

    pub fn snapshot_properties(&self) -> PreservedProperties {
        let mut properties = HashMap::new();
        for node in &self.nodes {
            snapshot(node, "", &mut properties);
        }
        properties
    }

    pub fn find(&self, key: &str) -> Option<&SceneNode> {
        self.iter().find(|node| node.key == key)
    }

    pub fn find_mut(&mut self, key: &str) -> Option<&mut SceneNode> {
        fn search<'a>(nodes: &'a mut [SceneNode], key: &str) -> Option<&'a mut SceneNode> {
            for node in nodes {
                if node.key == key {
                    return Some(node);
                }
                if let Some(found) = search(&mut node.children, key) {
                    return Some(found);
                }
            }
            None
        }
        search(&mut self.nodes, key)
    }

    /// All nodes, depth first.
    pub fn iter(&self) -> impl Iterator<Item = &SceneNode> {
        let mut all = Vec::new();
        for node in &self.nodes {
            node.visit(&mut |n| all.push(n));
        }
        all.into_iter()
    }

    /// Visible nodes with geometry, with their effective visibility (a hidden
    /// parent hides its children).
    pub fn visible_nodes(&self) -> Vec<&SceneNode> {
        fn collect<'a>(nodes: &'a [SceneNode], out: &mut Vec<&'a SceneNode>) {
            for node in nodes.iter().filter(|n| n.visible) {
                out.push(node);
                collect(&node.children, out);
            }
        }
        let mut out = Vec::new();
        collect(&self.nodes, &mut out);
        out
    }

    /// Bounds of everything visible except the helpers.
    pub fn visible_bounds(&self) -> Aabb {
        let mut bounds = self
            .visible_nodes()
            .iter()
            .fold(Aabb::EMPTY, |b, node| b.union(node.own_bounds()));
        for mesh in &self.overlay.meshes {
            bounds = bounds.union(mesh.bounds);
        }
        bounds
    }

    /// Bounds of selected nodes and their subtrees.
    pub fn selection_bounds(&self) -> Aabb {
        self.selection
            .iter()
            .filter_map(|key| self.find(key))
            .fold(Aabb::EMPTY, |b, node| b.union(node.subtree_bounds()))
    }

    /// The axis helpers span twice the diagonal of the shown objects.
    pub fn axis_half_length(&self) -> f32 {
        let bounds = self
            .nodes
            .iter()
            .fold(Aabb::EMPTY, |b, n| b.union(n.subtree_bounds()));
        if bounds.is_empty() || bounds.diagonal() <= 0.0 {
            DEFAULT_AXIS_HALF_LENGTH
        } else {
            2.0 * bounds.diagonal()
        }
    }

    /// Shows or hides a node and its whole subtree.
    pub fn set_visible(&mut self, key: &str, visible: bool) {
        if let Some(node) = self.find_mut(key) {
            node.visit_mut(&mut |n| n.visible = visible);
        }
    }

    // -- selection ---------------------------------------------------------

    pub fn is_selected(&self, key: &str) -> bool {
        self.selection.iter().any(|k| k == key)
    }

    pub fn select_only(&mut self, key: &str) {
        self.selection = vec![key.to_owned()];
    }

    pub fn toggle_selected(&mut self, key: &str) {
        if let Some(index) = self.selection.iter().position(|k| k == key) {
            self.selection.remove(index);
        } else {
            self.selection.push(key.to_owned());
        }
    }

    pub fn current(&self) -> Option<&SceneNode> {
        self.selection.last().and_then(|key| self.find(key))
    }

    /// Keys of selected nodes that are not inside another selected node,
    /// in tree order.
    pub fn selected_roots(&self) -> Vec<NodeKey> {
        fn collect(nodes: &[SceneNode], scene: &Scene, out: &mut Vec<NodeKey>) {
            for node in nodes {
                if scene.is_selected(&node.key) {
                    out.push(node.key.clone());
                } else {
                    collect(&node.children, scene, out);
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.nodes, self, &mut out);
        out
    }

    /// Top-level keys, e.g. for exporting everything.
    pub fn top_level_keys(&self) -> Vec<NodeKey> {
        self.nodes.iter().map(|n| n.key.clone()).collect()
    }

    /// Top-level nodes that are selected or contain a selected node.
    pub fn selected_top_level(&self) -> Vec<NodeKey> {
        self.nodes
            .iter()
            .filter(|top| {
                let mut hit = false;
                top.visit(&mut |n| hit |= self.is_selected(&n.key));
                hit
            })
            .map(|n| n.key.clone())
            .collect()
    }

    fn prune_selection(&mut self) {
        let selection = std::mem::take(&mut self.selection);
        self.selection = selection
            .into_iter()
            .filter(|k| self.find(k).is_some())
            .collect();
    }
}

fn snapshot(node: &SceneNode, parent: &str, out: &mut PreservedProperties) {
    let path = format!("{parent}/{}", node.name);
    out.insert(path.clone(), (node.visible, node.overrides.clone()));
    for child in &node.children {
        snapshot(child, &path, out);
    }
}

fn restore(node: &mut SceneNode, parent: &str, preserved: &PreservedProperties) {
    let path = format!("{parent}/{}", node.name);
    if let Some((visible, overrides)) = preserved.get(&path) {
        node.visible = *visible;
        node.overrides = overrides.clone();
    }
    for child in &mut node.children {
        restore(child, &path, preserved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(key: &str, name: &str, children: Vec<SceneNode>) -> SceneNode {
        SceneNode {
            key: key.into(),
            name: name.into(),
            kind: "Workplane".into(),
            inspectable: false,
            parts: vec![Part {
                mesh: Arc::new(Mesh::lines(vec![[0.0; 3], [1.0, 2.0, 3.0]])),
                color: None,
                alpha: None,
            }],
            children,
            visible: true,
            color: None,
            alpha: None,
            edge_color: None,
            overrides: Overrides::default(),
        }
    }

    fn scene() -> Scene {
        let mut scene = Scene::default();
        scene.append(
            vec![
                node(
                    "a",
                    "assy",
                    vec![node("a1", "plate", vec![]), node("a2", "pin", vec![])],
                ),
                node("b", "box", vec![]),
            ],
            None,
        );
        scene
    }

    #[test]
    fn hiding_a_parent_hides_the_subtree() {
        let mut scene = scene();
        scene.set_visible("a", false);
        let visible: Vec<_> = scene
            .visible_nodes()
            .iter()
            .map(|n| n.key.as_str())
            .collect();
        assert_eq!(visible, ["b"]);
    }

    #[test]
    fn selection_roots_skip_nested_selections() {
        let mut scene = scene();
        scene.select_only("a2");
        scene.toggle_selected("a");
        scene.toggle_selected("b");
        assert_eq!(scene.selected_roots(), ["a", "b"]);
        assert_eq!(scene.selected_top_level(), ["a", "b"]);
        assert_eq!(scene.current().unwrap().key, "b");
    }

    #[test]
    fn properties_survive_a_reload_by_name() {
        let mut scene = scene();
        scene.set_visible("a1", false);
        scene.find_mut("b").unwrap().overrides.color = Some([1.0, 0.0, 0.0]);
        let preserved = scene.snapshot_properties();

        let removed = scene.replace(
            vec![
                node("x", "assy", vec![node("x1", "plate", vec![])]),
                node("y", "box", vec![]),
            ],
            Some(&preserved),
        );
        assert_eq!(removed, ["a", "b"]);
        assert!(!scene.find("x1").unwrap().visible);
        assert_eq!(
            scene.find("y").unwrap().overrides.color,
            Some([1.0, 0.0, 0.0])
        );
    }

    #[test]
    fn colors_resolve_in_priority_order() {
        let mut n = node("n", "n", vec![]);
        let default = [0.5; 3];
        assert_eq!(n.part_color(&n.parts[0], default), (default, 1.0));
        n.color = Some([0.0, 0.0, 1.0]);
        n.alpha = Some(0.5);
        assert_eq!(n.part_color(&n.parts[0], default), ([0.0, 0.0, 1.0], 0.5));
        n.overrides.color = Some([1.0, 0.0, 0.0]);
        assert_eq!(n.part_color(&n.parts[0], default).0, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn axis_helpers_scale_with_the_model() {
        let mut scene = Scene::default();
        assert_eq!(scene.axis_half_length(), DEFAULT_AXIS_HALF_LENGTH);
        scene.append(vec![node("b", "box", vec![])], None);
        let diagonal = Vec3::new(1.0, 2.0, 3.0).length();
        assert!((scene.axis_half_length() - 2.0 * diagonal).abs() < 1e-5);
    }
}
