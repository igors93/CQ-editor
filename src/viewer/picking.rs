//! Finding the object under the mouse pointer.
//!
//! Faces are hit-tested exactly with a ray. Topological edges and vertices
//! are matched in screen space within a few pixels, since a ray would
//! practically never touch them. A front face occludes entities behind it.

use glam::{Mat4, Vec2, Vec3};

use super::{SelectionMode, camera::Camera};
use crate::scene::{Mesh, TopologyEntity};

/// How close to an edge or point the pointer must be, in pixels.
const PICK_TOLERANCE_PX: f32 = 6.0;
/// NDC depth allowance for an edge or vertex lying on the hit face.
const OCCLUSION_EPSILON: f32 = 1e-3;

/// Something to test against, tagged with a caller-defined id.
pub struct Candidate<'a, T> {
    pub id: T,
    pub mesh: &'a Mesh,
    pub transform: Mat4,
}

/// Exact result of a view pick. `entity` is absent only for legacy meshes
/// without worker-supplied topology metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit<T> {
    pub id: T,
    pub entity: Option<TopologyEntity>,
    pub position: Vec3,
}

struct RankedHit<T> {
    hit: Hit<T>,
    depth: f32,
    screen_distance: f32,
    priority: u8,
    surface: bool,
}

/// Picks the candidate closest to the viewer under `pointer_ndc`
/// (normalized device coordinates; `viewport_px` converts to pixels).
pub fn pick<'a, T: Clone>(
    camera: &Camera,
    aspect: f32,
    pointer_ndc: Vec2,
    viewport_px: Vec2,
    mode: SelectionMode,
    candidates: impl IntoIterator<Item = Candidate<'a, T>>,
) -> Option<Hit<T>> {
    let (origin, dir) = camera.ray(pointer_ndc, aspect);
    let to_px = |ndc: Vec2| ndc * viewport_px * 0.5;
    let pointer_px = to_px(pointer_ndc);

    let mut hits = Vec::new();
    let mut front_surface: Option<f32> = None;

    for candidate in candidates {
        let mesh = candidate.mesh;
        if mesh.bounds.is_empty() {
            continue;
        }
        let inverse = candidate.transform.inverse();
        let local_origin = inverse.transform_point3(origin);
        let local_dir = inverse.transform_vector3(dir);

        if ray_hits_box(local_origin, local_dir, mesh.bounds.min, mesh.bounds.max) {
            let topology_faces = !mesh.topology.face_indices.is_empty();
            if let Some((distance, triangle)) = mesh.nearest_triangle(local_origin, local_dir) {
                let local_position = local_origin + local_dir * distance;
                let position = candidate.transform.transform_point3(local_position);
                if let Some(depth) = projected_depth(camera, aspect, position) {
                    front_surface = Some(front_surface.map_or(depth, |front| front.min(depth)));
                    let entity = topology_faces
                        .then(|| mesh.topology.triangle_faces.get(triangle).copied())
                        .flatten()
                        .map(TopologyEntity::Face);
                    if mode.allows_faces() && mode.accepts(entity) {
                        hits.push(RankedHit {
                            hit: Hit {
                                id: candidate.id.clone(),
                                entity: mode.entity(entity),
                                position,
                            },
                            depth,
                            screen_distance: 0.0,
                            priority: 2,
                            surface: true,
                        });
                    }
                }
            }
        }

        let screen = |p: &[f32; 3]| {
            let world = candidate.transform.transform_point3(Vec3::from_array(*p));
            camera
                .project(world, aspect)
                .map(|ndc| (to_px(ndc.truncate()), world, ndc.z))
        };
        for (segment_index, segment) in mesh
            .edges
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .filter(|_| mode.allows_edges())
        {
            let (Some((a, wa, _)), Some((b, wb, _))) = (screen(&segment[0]), screen(&segment[1]))
            else {
                continue;
            };
            let (distance, t) = distance_to_segment(pointer_px, a, b);
            if distance <= PICK_TOLERANCE_PX {
                let position = wa.lerp(wb, t);
                if let Some(depth) = projected_depth(camera, aspect, position) {
                    let entity = mesh
                        .topology
                        .segment_edges
                        .get(segment_index)
                        .copied()
                        .map(TopologyEntity::Edge);
                    if mode.accepts(entity) {
                        hits.push(RankedHit {
                            hit: Hit {
                                id: candidate.id.clone(),
                                entity: mode.entity(entity),
                                position,
                            },
                            depth,
                            screen_distance: distance,
                            priority: 1,
                            surface: false,
                        });
                    }
                }
            }
        }
        let topology_vertices = !mesh.topology.vertices.is_empty();
        let vertices = if topology_vertices {
            &mesh.topology.vertices
        } else {
            &mesh.points
        };
        for (index, point) in vertices
            .iter()
            .enumerate()
            .filter(|_| mode.allows_vertices())
        {
            if let Some((screen_pos, position, depth)) = screen(point)
                && let distance = screen_pos.distance(pointer_px)
                && distance <= PICK_TOLERANCE_PX
            {
                let entity = topology_vertices.then_some(TopologyEntity::Vertex(index as u32 + 1));
                if mode.accepts(entity) {
                    hits.push(RankedHit {
                        hit: Hit {
                            id: candidate.id.clone(),
                            entity: mode.entity(entity),
                            position,
                        },
                        depth,
                        screen_distance: distance,
                        priority: 0,
                        surface: false,
                    });
                }
            }
        }
    }

    hits.into_iter()
        .filter(|hit| {
            hit.surface || front_surface.is_none_or(|depth| hit.depth <= depth + OCCLUSION_EPSILON)
        })
        .min_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.depth.total_cmp(&b.depth))
                .then_with(|| a.screen_distance.total_cmp(&b.screen_distance))
        })
        .map(|ranked| ranked.hit)
}

fn projected_depth(camera: &Camera, aspect: f32, point: Vec3) -> Option<f32> {
    camera
        .project(point, aspect)
        .map(|projected| projected.z)
        .filter(|depth| (0.0..=1.0).contains(depth))
}

/// Slab test.
fn ray_hits_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> bool {
    let inv = dir.recip();
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let near = t1.min(t2).max_element();
    let far = t1.max(t2).min_element();
    // NaN (ray parallel to a slab, starting on its border) counts as a hit.
    !(near > far || far < 0.0)
}

/// Distance from `p` to segment `ab`, and the parameter of the closest point.
fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let length2 = ab.length_squared();
    let t = if length2 > 0.0 {
        ((p - a).dot(ab) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.distance(a + ab * t), t)
}

#[cfg(test)]
#[path = "../../tests/rust/viewer/test_picking.rs"]
mod tests;
