//! What the viewer shows: the objects of the last run, the axis helpers and
//! the inspector overlay.
//!
//! Nodes mirror the worker's scene nodes (one per shown object, assemblies as
//! subtrees). Display state that belongs to the GUI (visibility, color
//! overrides, selection) lives here too.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use glam::{Mat4, Vec3};

use crate::spatial::{BoundsBvh, TriangleBvh};
use crate::worker::protocol::{self, RequestId};

pub type NodeKey = String;

/// sRGB color with components in 0..1.
pub type Rgb = [f32; 3];

/// A 1-based OCCT subshape identifier supplied by the worker.
///
/// It is valid only for the current live scene entry. Rebuilding/remodeling
/// the object may assign the number to a different subshape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TopologyEntity {
    Face(u32),
    Edge(u32),
    Vertex(u32),
}

impl TopologyEntity {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Face(_) => "Face",
            Self::Edge(_) => "Edge",
            Self::Vertex(_) => "Vertex",
        }
    }

    pub fn identifier(self) -> u32 {
        match self {
            Self::Face(id) | Self::Edge(id) | Self::Vertex(id) => id,
        }
    }

    pub fn topology_kind(self) -> protocol::TopologyKind {
        match self {
            Self::Face(_) => protocol::TopologyKind::Face,
            Self::Edge(_) => protocol::TopologyKind::Edge,
            Self::Vertex(_) => protocol::TopologyKind::Vertex,
        }
    }
}

/// The exact CAD entity selected in one displayed part of a scene node.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologySelection {
    pub node: NodeKey,
    pub part: usize,
    pub entity: TopologyEntity,
    pub position: Vec3,
}

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

    pub fn transformed(self, transform: Mat4) -> Aabb {
        if self.is_empty() {
            return self;
        }
        Aabb::from_points((0..8).map(|corner| {
            transform.transform_point3(Vec3::new(
                if corner & 1 == 0 {
                    self.min.x
                } else {
                    self.max.x
                },
                if corner & 2 == 0 {
                    self.min.y
                } else {
                    self.max.y
                },
                if corner & 4 == 0 {
                    self.min.z
                } else {
                    self.max.z
                },
            ))
        }))
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
    pub topology: MeshTopology,
    pub bounds: Aabb,
    triangle_bvh: OnceLock<TriangleBvh>,
    edge_bvh: OnceLock<BoundsBvh>,
    vertex_bvh: OnceLock<BoundsBvh>,
}

/// Pick geometry and its association with the worker's OCCT topology index.
#[derive(Debug, Default)]
pub struct MeshTopology {
    /// Separate pick triangles; normally empty, and the display triangles
    /// are picked instead.
    pub face_positions: Vec<[f32; 3]>,
    pub face_indices: Vec<u32>,
    /// One 1-based face id per pick triangle: per triangle of
    /// `face_indices` when present, otherwise of [`Mesh::indices`].
    pub triangle_faces: Vec<u32>,
    /// One 1-based edge id per segment in [`Mesh::edges`].
    pub segment_edges: Vec<u32>,
    /// All OCCT vertices in 1-based id order, including vertices of solids.
    pub vertices: Vec<[f32; 3]>,
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
            topology: MeshTopology::default(),
            bounds,
            triangle_bvh: OnceLock::new(),
            edge_bvh: OnceLock::new(),
            vertex_bvh: OnceLock::new(),
        }
    }

    /// Only edges, e.g. for helper lines and grids.
    pub fn lines(segments: Vec<[f32; 3]>) -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new(), segments, Vec::new())
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bytes held by the vertex, index and topology arrays.
    pub fn byte_size(&self) -> usize {
        let topology = &self.topology;
        let points = self.positions.len()
            + self.normals.len()
            + self.edges.len()
            + self.points.len()
            + topology.face_positions.len()
            + topology.vertices.len();
        let integers = self.indices.len()
            + topology.face_indices.len()
            + topology.triangle_faces.len()
            + topology.segment_edges.len();
        points * size_of::<[f32; 3]>() + integers * size_of::<u32>()
    }

    fn pick_triangles(&self) -> (&[[f32; 3]], &[u32]) {
        if self.topology.face_indices.is_empty() {
            (&self.positions, &self.indices)
        } else {
            (&self.topology.face_positions, &self.topology.face_indices)
        }
    }

    /// The nearest triangle hit with a ray parameter in `range`.
    pub(crate) fn nearest_triangle_within(
        &self,
        origin: Vec3,
        direction: Vec3,
        range: (f32, f32),
    ) -> Option<(f32, usize)> {
        let (positions, indices) = self.pick_triangles();
        self.triangle_bvh
            .get_or_init(|| TriangleBvh::build(positions, indices))
            .nearest_within(origin, direction, positions, indices, range)
            .0
    }

    pub(crate) fn edge_bvh(&self) -> &BoundsBvh {
        self.edge_bvh.get_or_init(|| {
            BoundsBvh::build(self.edges.as_chunks::<2>().0.iter().map(|[a, b]| {
                let (a, b) = (Vec3::from_array(*a), Vec3::from_array(*b));
                (a.min(b), a.max(b))
            }))
        })
    }

    pub(crate) fn pick_vertices(&self) -> &[[f32; 3]] {
        if self.topology.vertices.is_empty() {
            &self.points
        } else {
            &self.topology.vertices
        }
    }

    pub(crate) fn vertex_bvh(&self) -> &BoundsBvh {
        self.vertex_bvh.get_or_init(|| {
            BoundsBvh::build(self.pick_vertices().iter().map(|point| {
                let point = Vec3::from_array(*point);
                (point, point)
            }))
        })
    }

    #[cfg(test)]
    pub(crate) fn nearest_triangle_with_stats(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> (Option<(f32, usize)>, usize) {
        let (positions, indices) = self.pick_triangles();
        self.triangle_bvh
            .get_or_init(|| TriangleBvh::build(positions, indices))
            .nearest_with_count(origin, direction, positions, indices)
    }

    /// Builds a small render mesh that highlights exactly one CAD entity.
    pub fn topology_mesh(&self, entity: TopologyEntity) -> Option<Mesh> {
        match entity {
            TopologyEntity::Face(identifier) => {
                let (pick_positions, pick_indices) = self.pick_triangles();
                let mut positions = Vec::new();
                let mut normals = Vec::new();
                let mut indices = Vec::new();
                for (triangle, &face) in pick_indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .zip(&self.topology.triangle_faces)
                {
                    if face != identifier {
                        continue;
                    }
                    let [Some(a), Some(b), Some(c)] =
                        triangle.map(|index| pick_positions.get(index as usize).copied())
                    else {
                        continue;
                    };
                    let normal = (Vec3::from_array(b) - Vec3::from_array(a))
                        .cross(Vec3::from_array(c) - Vec3::from_array(a))
                        .normalize_or(Vec3::Z)
                        .to_array();
                    let start = positions.len() as u32;
                    positions.extend([a, b, c]);
                    normals.extend([normal; 3]);
                    indices.extend([start, start + 1, start + 2]);
                }
                (!indices.is_empty())
                    .then(|| Mesh::new(positions, normals, indices, Vec::new(), Vec::new()))
            }
            TopologyEntity::Edge(identifier) => {
                let mut segments = Vec::new();
                for (segment, &edge) in self
                    .edges
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .zip(&self.topology.segment_edges)
                {
                    if edge == identifier {
                        segments.extend(*segment);
                    }
                }
                (!segments.is_empty()).then(|| Mesh::lines(segments))
            }
            TopologyEntity::Vertex(identifier) => self
                .topology
                .vertices
                .get(identifier.checked_sub(1)? as usize)
                .copied()
                .map(|point| {
                    Mesh::new(Vec::new(), Vec::new(), Vec::new(), Vec::new(), vec![point])
                }),
        }
    }
}

impl From<protocol::Mesh> for Mesh {
    fn from(mesh: protocol::Mesh) -> Self {
        let mut normals = mesh.normals;
        normals.resize(mesh.positions.len(), [0.0, 0.0, 1.0]);
        let bounds = mesh.bbox.map_or_else(
            || {
                Aabb::from_points(
                    mesh.positions
                        .iter()
                        .chain(&mesh.edges)
                        .chain(&mesh.points)
                        .map(|point| Vec3::from_array(*point)),
                )
            },
            |[min, max]| Aabb {
                min: Vec3::from_array(min),
                max: Vec3::from_array(max),
            },
        );
        let topology = MeshTopology {
            face_positions: mesh.topology.face_positions,
            face_indices: mesh.topology.face_indices,
            triangle_faces: mesh.topology.triangle_faces,
            segment_edges: mesh.topology.segment_edges,
            vertices: mesh.topology.vertices,
        };
        Mesh {
            // The worker already measured the geometry.
            bounds,
            topology,
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

#[derive(Clone, Debug)]
pub struct Part {
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
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
    /// The script line that created the object, 1-based, in `script`.
    pub line: Option<usize>,
    /// The script as the run that made this node executed it.
    pub script: Option<Arc<RunScript>>,
}

/// The script a run executed: the lines of its objects refer to this text,
/// which the editor may have changed since.
#[derive(Debug)]
pub struct RunScript {
    /// The document that ran; lines of another one mean nothing in it.
    pub document: u64,
    pub text: String,
}

impl SceneNode {
    /// Gives the node and its subtree the script their lines refer to.
    pub fn attach_script(&mut self, script: &Arc<RunScript>) {
        self.visit_mut(&mut |node| node.script = Some(script.clone()));
    }

    fn from_message(
        message: protocol::SceneNode,
        library: &mut MeshLibrary,
        request: Option<RequestId>,
    ) -> Result<Self, String> {
        let mut parts = Vec::with_capacity(message.parts.len());
        for part in message.parts {
            parts.push(Part {
                mesh: library.resolve(part.mesh, request)?,
                transform: Mat4::from_cols_array(&part.transform),
                color: part.color,
                alpha: part.alpha,
            });
        }
        let mut children = Vec::with_capacity(message.children.len());
        for child in message.children {
            children.push(Self::from_message(child, library, request)?);
        }
        Ok(Self {
            key: message.key,
            name: message.name,
            kind: message.kind,
            inspectable: message.inspectable,
            parts,
            children,
            visible: true,
            color: message.color,
            alpha: message.alpha,
            edge_color: message.edge_color,
            overrides: Overrides::default(),
            line: message.line,
            script: None,
        })
    }

    /// Bounds of this node's own parts (not its children).
    pub fn own_bounds(&self) -> Aabb {
        self.parts.iter().fold(Aabb::EMPTY, |b, part| {
            b.union(part.mesh.bounds.transformed(part.transform))
        })
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

/// Entities Ctrl+click can gather, e.g. for one selector of many edges.
pub const MAX_TOPOLOGY_SELECTIONS: usize = 64;

/// Default half length of the axis helpers when nothing is shown.
const DEFAULT_AXIS_HALF_LENGTH: f32 = 100.0;

/// Bytes of recently used worker meshes kept alive after the scene drops
/// them. The renderer keeps their GPU buffers for as long.
pub const RETAINED_MESH_BYTES: usize = 128 * 1024 * 1024;

/// Worker meshes by content-addressed geometry id.
///
/// The worker answers a request with references to meshes the request said
/// the GUI holds (`known_geometry`) and to meshes it sent earlier in the same
/// request, such as a previous pause of a debugging session. Both are pinned
/// until the request ends, whatever the scene drops meanwhile. Recently used
/// meshes are also retained within a byte budget, so the next run, a
/// debugging session or a render after Clear reuses them, and their GPU
/// buffers, instead of transferring and uploading them again.
pub struct MeshLibrary {
    meshes: HashMap<String, Weak<Mesh>>,
    /// Strong references by geometry id with their last use.
    recent: HashMap<String, (Arc<Mesh>, u64)>,
    recent_bytes: usize,
    budget: usize,
    clock: u64,
    /// Meshes a running request may still reference, by [`Mesh::id`].
    pins: HashMap<RequestId, HashMap<u64, Arc<Mesh>>>,
}

impl Default for MeshLibrary {
    fn default() -> Self {
        Self::with_budget(RETAINED_MESH_BYTES)
    }
}

impl MeshLibrary {
    pub fn with_budget(budget: usize) -> Self {
        Self {
            meshes: HashMap::new(),
            recent: HashMap::new(),
            recent_bytes: 0,
            budget,
            clock: 0,
            pins: HashMap::new(),
        }
    }

    /// Geometry ids the worker may send as references, sorted.
    pub fn known(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .meshes
            .iter()
            .filter(|(_, mesh)| mesh.strong_count() > 0)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Keeps the meshes of `ids` alive until `request` is released.
    pub fn pin(&mut self, request: RequestId, ids: &[String]) {
        let pinned = self.pins.entry(request).or_default();
        for mesh in ids
            .iter()
            .filter_map(|id| self.meshes.get(id).and_then(Weak::upgrade))
        {
            pinned.insert(mesh.id, mesh);
        }
    }

    /// Unpins every request for which `active` is false.
    pub fn release_pins(&mut self, mut active: impl FnMut(RequestId) -> bool) {
        let before = self.pins.len();
        self.pins.retain(|request, _| active(*request));
        if self.pins.len() != before {
            self.meshes.retain(|_, mesh| mesh.strong_count() > 0);
        }
    }

    /// The shared mesh for a worker message: a reference, a full mesh, or a
    /// full mesh whose geometry is already held, which keeps its GPU buffers.
    fn resolve(
        &mut self,
        message: protocol::Mesh,
        request: Option<RequestId>,
    ) -> Result<Arc<Mesh>, String> {
        let id = message.id.clone();
        let held = (!id.is_empty())
            .then(|| self.meshes.get(&id).and_then(Weak::upgrade))
            .flatten();
        let mesh = match held {
            Some(mesh) => mesh,
            None if message.bbox.is_some() => {
                let mesh = Arc::new(Mesh::from(message));
                if !id.is_empty() {
                    self.meshes.insert(id.clone(), Arc::downgrade(&mesh));
                }
                mesh
            }
            None => return Err(format!("worker referenced unavailable mesh {id}")),
        };
        if !id.is_empty() {
            if let Some(request) = request {
                self.pins
                    .entry(request)
                    .or_default()
                    .insert(mesh.id, mesh.clone());
            }
            self.touch(id, &mesh);
        }
        Ok(mesh)
    }

    fn touch(&mut self, id: String, mesh: &Arc<Mesh>) {
        self.clock += 1;
        if let Some(entry) = self.recent.get_mut(&id) {
            entry.1 = self.clock;
        } else {
            self.recent_bytes += mesh.byte_size();
            self.recent.insert(id, (mesh.clone(), self.clock));
        }
    }

    /// Drops the least recently used meshes beyond the budget.
    fn evict(&mut self) {
        if self.recent_bytes <= self.budget {
            return;
        }
        let mut by_age: Vec<(u64, String, usize)> = self
            .recent
            .iter()
            .map(|(id, (mesh, used))| (*used, id.clone(), mesh.byte_size()))
            .collect();
        by_age.sort_unstable();
        for (_, id, bytes) in by_age {
            if self.recent_bytes <= self.budget {
                break;
            }
            self.recent.remove(&id);
            self.recent_bytes -= bytes;
        }
        self.meshes.retain(|_, mesh| mesh.strong_count() > 0);
    }
}

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub axes: [AxisHelper; 3],
    /// Selected node keys; the last one is the "current" object.
    pub selection: Vec<NodeKey>,
    /// Exact entities, at most [`MAX_TOPOLOGY_SELECTIONS`], oldest first.
    /// Measurements relate the last two; selectors describe them all.
    pub topology_selections: Vec<TopologySelection>,
    topology_highlights: Vec<Part>,
    pub overlay: Overlay,
    meshes: MeshLibrary,
    /// Changes only when the set, placement or visibility of pickable parts
    /// changes. Camera and selection changes deliberately leave it alone.
    pick_revision: u64,
}

/// Process-wide, so two scenes never share a revision and an index built
/// for one scene is never mistaken for an index of another.
fn next_pick_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
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
            topology_selections: Vec::new(),
            topology_highlights: Vec::new(),
            overlay: Overlay::default(),
            meshes: MeshLibrary::default(),
            pick_revision: next_pick_revision(),
        }
    }
}

/// Visibility and overrides of nodes, keyed by their name path, so they can
/// be carried over to the next run.
pub type PreservedProperties = HashMap<String, (bool, Overrides)>;

impl Scene {
    /// Resolves full content-addressed meshes and references in one worker
    /// response to `request`, whose meshes stay pinned until it ends.
    pub fn decode_nodes(
        &mut self,
        request: Option<RequestId>,
        messages: Vec<protocol::SceneNode>,
    ) -> Result<Vec<SceneNode>, String> {
        let nodes = messages
            .into_iter()
            .map(|message| SceneNode::from_message(message, &mut self.meshes, request))
            .collect();
        self.meshes.evict();
        nodes
    }

    /// Geometry the worker may send as references: everything in the scene
    /// and the recently used meshes the library still retains.
    pub fn known_geometry(&self) -> Vec<String> {
        self.meshes.known()
    }

    /// Guarantees that `request` can reference `ids` until it is released.
    pub fn pin_geometry(&mut self, request: RequestId, ids: &[String]) {
        self.meshes.pin(request, ids);
    }

    /// Ends the guarantee of [`Scene::pin_geometry`] and of decoded meshes
    /// for every request that is no longer `active`.
    pub fn release_geometry(&mut self, active: impl FnMut(RequestId) -> bool) {
        self.meshes.release_pins(active);
    }

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
        self.bump_pick_revision();
    }

    /// Removes all objects; returns the removed top-level keys.
    pub fn clear(&mut self) -> Vec<NodeKey> {
        self.clear_selection();
        let removed: Vec<_> = self.nodes.drain(..).map(|n| n.key).collect();
        if !removed.is_empty() {
            self.bump_pick_revision();
        }
        removed
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
        if !removed.is_empty() {
            self.bump_pick_revision();
        }
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
        let mut changed = false;
        if let Some(node) = self.find_mut(key) {
            node.visit_mut(&mut |n| {
                changed |= n.visible != visible;
                n.visible = visible;
            });
        }
        if changed {
            self.bump_pick_revision();
        }
    }

    pub(crate) fn pick_revision(&self) -> u64 {
        self.pick_revision
    }

    fn bump_pick_revision(&mut self) {
        self.pick_revision = next_pick_revision();
    }

    // -- selection ---------------------------------------------------------

    pub fn is_selected(&self, key: &str) -> bool {
        self.selection.iter().any(|k| k == key)
    }

    pub fn select_only(&mut self, key: &str) {
        self.clear_topology_selection();
        self.selection = vec![key.to_owned()];
    }

    pub fn toggle_selected(&mut self, key: &str) {
        self.clear_topology_selection();
        if let Some(index) = self.selection.iter().position(|k| k == key) {
            self.selection.remove(index);
        } else {
            self.selection.push(key.to_owned());
        }
    }

    pub fn ensure_selected(&mut self, key: &str) {
        if let Some(index) = self.selection.iter().position(|candidate| candidate == key) {
            self.selection.remove(index);
        }
        self.selection.push(key.to_owned());
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.clear_topology_selection();
    }

    pub fn select_topology(&mut self, selection: TopologySelection, extend: bool) -> bool {
        let highlight = self
            .find(&selection.node)
            .and_then(|node| node.parts.get(selection.part))
            .and_then(|part| {
                part.mesh.topology_mesh(selection.entity).map(|mesh| Part {
                    mesh: Arc::new(mesh),
                    transform: part.transform,
                    color: None,
                    alpha: None,
                })
            });
        let Some(highlight) = highlight else {
            if !extend {
                self.clear_topology_selection();
            }
            return false;
        };

        let same_entity = |candidate: &TopologySelection| {
            candidate.node == selection.node
                && candidate.part == selection.part
                && candidate.entity == selection.entity
        };
        if extend && let Some(index) = self.topology_selections.iter().position(same_entity) {
            self.topology_selections.remove(index);
            self.topology_highlights.remove(index);
            return true;
        }
        if !extend {
            self.clear_topology_selection();
        } else if self.topology_selections.len() == MAX_TOPOLOGY_SELECTIONS {
            self.topology_selections.remove(0);
            self.topology_highlights.remove(0);
        }
        self.topology_selections.push(selection);
        self.topology_highlights.push(highlight);
        true
    }

    pub fn clear_topology_selection(&mut self) {
        self.topology_selections.clear();
        self.topology_highlights.clear();
    }

    pub fn current_topology(&self) -> Option<&TopologySelection> {
        self.topology_selections.last()
    }

    pub fn topology_highlights(&self) -> &[Part] {
        &self.topology_highlights
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
        let selected: HashSet<_> = self.selection.iter().cloned().collect();
        let mut index = 0;
        self.topology_selections.retain(|selection| {
            let keep = selected.contains(&selection.node);
            if !keep {
                self.topology_highlights.remove(index);
            } else {
                index += 1;
            }
            keep
        });
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
#[path = "../tests/rust/scene/test_scene.rs"]
mod tests;
