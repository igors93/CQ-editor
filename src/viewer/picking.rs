//! Finding the object under the mouse pointer.
//!
//! Faces are hit-tested exactly with a ray. Edges and vertices of objects
//! without faces (wires, sketches, `.vertices()` results) are matched in
//! screen space within a few pixels, since a ray would practically never
//! touch them.

use glam::{Vec2, Vec3};

use super::camera::Camera;
use crate::scene::Mesh;

/// How close to an edge or point the pointer must be, in pixels.
const PICK_TOLERANCE_PX: f32 = 6.0;

/// Something to test against, tagged with a caller-defined id.
pub struct Candidate<'a, T> {
    pub id: T,
    pub mesh: &'a Mesh,
}

/// Picks the candidate closest to the viewer under `pointer_ndc`
/// (normalized device coordinates; `viewport_px` converts to pixels).
pub fn pick<'a, T: Clone>(
    camera: &Camera,
    aspect: f32,
    pointer_ndc: Vec2,
    viewport_px: Vec2,
    candidates: impl IntoIterator<Item = Candidate<'a, T>>,
) -> Option<T> {
    let (origin, dir) = camera.ray(pointer_ndc, aspect);
    let to_px = |ndc: Vec2| ndc * viewport_px * 0.5;
    let pointer_px = to_px(pointer_ndc);

    let mut best: Option<(f32, T)> = None;
    let mut consider = |depth: f32, id: &T| {
        if best.as_ref().is_none_or(|(d, _)| depth < *d) {
            best = Some((depth, id.clone()));
        }
    };

    for candidate in candidates {
        let mesh = candidate.mesh;
        if mesh.bounds.is_empty() {
            continue;
        }

        if !mesh.indices.is_empty() {
            if ray_hits_box(origin, dir, mesh.bounds.min, mesh.bounds.max)
                && let Some(t) = nearest_triangle(origin, dir, mesh)
            {
                consider(t, &candidate.id);
            }
            continue; // edges of solids are not picked separately
        }

        let screen = |p: &[f32; 3]| {
            camera
                .project(Vec3::from_array(*p), aspect)
                .map(|ndc| (to_px(ndc.truncate()), Vec3::from_array(*p)))
        };
        for segment in mesh.edges.as_chunks::<2>().0 {
            let (Some((a, wa)), Some((b, wb))) = (screen(&segment[0]), screen(&segment[1])) else {
                continue;
            };
            let (distance, t) = distance_to_segment(pointer_px, a, b);
            if distance <= PICK_TOLERANCE_PX {
                let point = wa.lerp(wb, t);
                consider((point - origin).dot(dir), &candidate.id);
            }
        }
        for p in &mesh.points {
            if let Some((screen_pos, world)) = screen(p)
                && screen_pos.distance(pointer_px) <= PICK_TOLERANCE_PX
            {
                consider((world - origin).dot(dir), &candidate.id);
            }
        }
    }
    best.map(|(_, id)| id)
}

fn nearest_triangle(origin: Vec3, dir: Vec3, mesh: &Mesh) -> Option<f32> {
    let vertex = |i: u32| Vec3::from_array(mesh.positions[i as usize]);
    mesh.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|tri| ray_triangle(origin, dir, vertex(tri[0]), vertex(tri[1]), vertex(tri[2])))
        .min_by(f32::total_cmp)
}

/// Möller–Trumbore intersection, accepting both triangle sides; returns the
/// distance along the ray.
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = dir.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(ab);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(q) * inv;
    (t > 0.0).then_some(t)
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
mod tests {
    use super::*;
    use crate::viewer::camera::StandardView;

    fn square(z: f32, size: f32) -> Mesh {
        Mesh::new(
            vec![
                [-size, -size, z],
                [size, -size, z],
                [size, size, z],
                [-size, size, z],
            ],
            vec![[0.0, 0.0, 1.0]; 4],
            vec![0, 1, 2, 0, 2, 3],
            Vec::new(),
            Vec::new(),
        )
    }

    fn top_camera() -> Camera {
        Camera {
            rotation: StandardView::Top.rotation(),
            extent: 10.0,
            ..Camera::default()
        }
    }

    #[test]
    fn picks_the_nearest_face() {
        let (low, high) = (square(0.0, 5.0), square(3.0, 1.0));
        let candidates = || {
            [
                Candidate {
                    id: "low",
                    mesh: &low,
                },
                Candidate {
                    id: "high",
                    mesh: &high,
                },
            ]
        };
        let camera = top_camera();
        let viewport = Vec2::new(800.0, 800.0);
        assert_eq!(
            pick(&camera, 1.0, Vec2::ZERO, viewport, candidates()),
            Some("high")
        );
        assert_eq!(
            pick(&camera, 1.0, Vec2::new(0.3, 0.3), viewport, candidates()),
            Some("low")
        );
        assert_eq!(
            pick(&camera, 1.0, Vec2::new(0.9, 0.9), viewport, candidates()),
            None
        );
    }

    #[test]
    fn picks_wires_within_tolerance() {
        let wire = Mesh::lines(vec![[-5.0, 0.0, 0.0], [5.0, 0.0, 0.0]]);
        let camera = top_camera();
        let viewport = Vec2::new(1000.0, 1000.0);
        // 1 world unit = 50 px here; 0.1 units off the line is 5 px.
        let near = Vec2::new(0.0, 0.1 / camera.extent);
        let far = Vec2::new(0.0, 0.5 / camera.extent);
        let candidates = |m| [Candidate { id: 1, mesh: m }];
        assert_eq!(
            pick(&camera, 1.0, near, viewport, candidates(&wire)),
            Some(1)
        );
        assert_eq!(pick(&camera, 1.0, far, viewport, candidates(&wire)), None);
    }
}
