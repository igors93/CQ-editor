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
