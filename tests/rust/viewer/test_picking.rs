use super::*;
use crate::viewer::camera::StandardView;

fn square(z: f32, size: f32) -> Mesh {
    let mut mesh = Mesh::new(
        vec![
            [-size, -size, z],
            [size, -size, z],
            [size, size, z],
            [-size, size, z],
        ],
        vec![[0.0, 0.0, 1.0]; 4],
        vec![0, 1, 2, 0, 2, 3],
        vec![
            [-size, -size, z],
            [size, -size, z],
            [size, -size, z],
            [size, size, z],
            [size, size, z],
            [-size, size, z],
            [-size, size, z],
            [-size, -size, z],
        ],
        Vec::new(),
    );
    mesh.topology.face_positions = mesh.positions.clone();
    mesh.topology.face_indices = mesh.indices.clone();
    mesh.topology.triangle_faces = vec![1, 1];
    mesh.topology.segment_edges = vec![1, 2, 3, 4];
    mesh.topology.vertices = mesh.positions.clone();
    mesh
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
                transform: Mat4::IDENTITY,
            },
            Candidate {
                id: "high",
                mesh: &high,
                transform: Mat4::IDENTITY,
            },
        ]
    };
    let camera = top_camera();
    let viewport = Vec2::new(800.0, 800.0);
    assert_eq!(
        pick(
            &camera,
            1.0,
            Vec2::ZERO,
            viewport,
            SelectionMode::Auto,
            candidates(),
        )
        .map(|hit| (hit.id, hit.entity)),
        Some(("high", Some(TopologyEntity::Face(1))))
    );
    assert_eq!(
        pick(
            &camera,
            1.0,
            Vec2::new(0.3, 0.3),
            viewport,
            SelectionMode::Auto,
            candidates(),
        )
        .map(|hit| (hit.id, hit.entity)),
        Some(("low", Some(TopologyEntity::Face(1))))
    );
    assert_eq!(
        pick(
            &camera,
            1.0,
            Vec2::new(0.9, 0.9),
            viewport,
            SelectionMode::Auto,
            candidates(),
        ),
        None
    );
}

#[test]
fn picks_wires_within_tolerance() {
    let wire = Mesh::lines(vec![[-5.0, 0.0, 0.0], [5.0, 0.0, 0.0]]);
    let mut wire = wire;
    wire.topology.segment_edges = vec![7];
    let camera = top_camera();
    let viewport = Vec2::new(1000.0, 1000.0);
    // 1 world unit = 50 px here; 0.1 units off the line is 5 px.
    let near = Vec2::new(0.0, 0.1 / camera.extent);
    let far = Vec2::new(0.0, 0.5 / camera.extent);
    let candidates = |m| {
        [Candidate {
            id: 1,
            mesh: m,
            transform: Mat4::IDENTITY,
        }]
    };
    assert_eq!(
        pick(
            &camera,
            1.0,
            near,
            viewport,
            SelectionMode::Auto,
            candidates(&wire),
        )
        .map(|hit| (hit.id, hit.entity)),
        Some((1, Some(TopologyEntity::Edge(7))))
    );
    assert_eq!(
        pick(
            &camera,
            1.0,
            far,
            viewport,
            SelectionMode::Auto,
            candidates(&wire),
        ),
        None
    );
}

#[test]
fn prioritizes_vertices_then_edges_then_faces() {
    let mesh = square(0.0, 5.0);
    let camera = top_camera();
    let viewport = Vec2::splat(1000.0);
    let pick_entity = |pointer| {
        pick(
            &camera,
            1.0,
            pointer,
            viewport,
            SelectionMode::Auto,
            [Candidate {
                id: (),
                mesh: &mesh,
                transform: Mat4::IDENTITY,
            }],
        )
        .and_then(|hit| hit.entity)
    };

    assert_eq!(pick_entity(Vec2::ZERO), Some(TopologyEntity::Face(1)));
    assert_eq!(
        pick_entity(Vec2::new(0.0, -0.5)),
        Some(TopologyEntity::Edge(1))
    );
    assert_eq!(
        pick_entity(Vec2::new(0.5, 0.5)),
        Some(TopologyEntity::Vertex(3))
    );
}

#[test]
fn a_front_face_occludes_a_back_edge() {
    let face = square(3.0, 2.0);
    let mut back_edge = Mesh::lines(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
    back_edge.topology.segment_edges = vec![9];
    let camera = top_camera();
    let hit = pick(
        &camera,
        1.0,
        Vec2::ZERO,
        Vec2::splat(800.0),
        SelectionMode::Auto,
        [
            Candidate {
                id: "face",
                mesh: &face,
                transform: Mat4::IDENTITY,
            },
            Candidate {
                id: "edge",
                mesh: &back_edge,
                transform: Mat4::IDENTITY,
            },
        ],
    )
    .unwrap();
    assert_eq!(hit.id, "face");
    assert_eq!(hit.entity, Some(TopologyEntity::Face(1)));
}

#[test]
fn picks_a_transformed_mesh_in_world_space() {
    let mesh = square(0.0, 1.0);
    let hit = pick(
        &top_camera(),
        1.0,
        Vec2::ZERO,
        Vec2::splat(800.0),
        SelectionMode::Auto,
        [Candidate {
            id: "moved",
            mesh: &mesh,
            transform: Mat4::from_translation(Vec3::new(0.0, 0.0, 4.0)),
        }],
    )
    .unwrap();
    assert_eq!(hit.id, "moved");
    assert!((hit.position.z - 4.0).abs() < 1e-5);
}

#[test]
fn explicit_selection_modes_only_return_the_requested_topology() {
    let mesh = square(0.0, 5.0);
    let camera = top_camera();
    let viewport = Vec2::splat(1000.0);
    let pick_entity = |pointer, mode| {
        pick(
            &camera,
            1.0,
            pointer,
            viewport,
            mode,
            [Candidate {
                id: (),
                mesh: &mesh,
                transform: Mat4::IDENTITY,
            }],
        )
        .map(|hit| hit.entity)
    };

    let corner = Vec2::new(0.5, 0.5);
    assert_eq!(
        pick_entity(corner, SelectionMode::Vertex),
        Some(Some(TopologyEntity::Vertex(3)))
    );
    assert!(matches!(
        pick_entity(corner, SelectionMode::Edge),
        Some(Some(TopologyEntity::Edge(_)))
    ));
    assert_eq!(
        pick_entity(Vec2::ZERO, SelectionMode::Face),
        Some(Some(TopologyEntity::Face(1)))
    );
    assert_eq!(
        pick_entity(corner, SelectionMode::Object),
        Some(None),
        "object mode must not leak a subshape id"
    );
}

#[test]
fn triangle_bvh_prunes_a_large_mesh() {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for y in 0..64 {
        for x in 0..64 {
            let start = positions.len() as u32;
            positions.extend([
                [x as f32, y as f32, 0.0],
                [x as f32 + 0.8, y as f32, 0.0],
                [x as f32, y as f32 + 0.8, 0.0],
            ]);
            indices.extend([start, start + 1, start + 2]);
        }
    }
    let mesh = Mesh::new(
        positions.clone(),
        vec![[0.0, 0.0, 1.0]; positions.len()],
        indices,
        Vec::new(),
        Vec::new(),
    );
    let (hit, tested) = mesh.nearest_triangle_with_stats(Vec3::new(32.2, 32.2, 10.0), Vec3::NEG_Z);

    assert!(hit.is_some());
    assert!(tested <= 16, "BVH tested {tested} of 4096 triangles");
}
