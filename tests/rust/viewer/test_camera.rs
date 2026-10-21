use super::*;

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-4
}

fn looking(view: StandardView) -> Camera {
    Camera {
        rotation: view.rotation(),
        ..Camera::default()
    }
}

#[test]
fn standard_views_look_where_expected() {
    let mut camera = looking(StandardView::Top);
    assert!(close(camera.forward(), Vec3::NEG_Z));
    assert!(close(camera.up(), Vec3::Y));
    assert!(close(camera.right(), Vec3::X));

    camera.rotation = StandardView::Front.rotation();
    assert!(close(camera.forward(), Vec3::Y));
    assert!(close(camera.up(), Vec3::Z));

    // From below, X still points right.
    camera.rotation = StandardView::Bottom.rotation();
    assert!(close(camera.right(), Vec3::X));
}

#[test]
fn turntable_keeps_z_up() {
    let mut camera = looking(StandardView::Front);
    camera.orbit_turntable(Vec2::new(120.0, 0.0));
    assert!(close(camera.up(), Vec3::Z));
    camera.orbit_turntable(Vec2::new(0.0, 50.0));
    assert!(camera.up().z > 0.5);
    assert!(camera.right().z.abs() < 1e-5);
}

#[test]
fn zoom_keeps_the_anchor_fixed() {
    let mut camera = Camera::default();
    let anchor = Vec2::new(0.5, -0.25);
    let aspect = 1.5;
    let (before, _) = camera.ray(anchor, aspect);
    let before_on_plane =
        before + camera.forward() * (camera.target - before).dot(camera.forward());
    camera.zoom(0.5, anchor, aspect);
    let (after, _) = camera.ray(anchor, aspect);
    let after_on_plane = after + camera.forward() * (camera.target - after).dot(camera.forward());
    assert!(close(before_on_plane, after_on_plane));
    assert!((camera.extent - 10.0).abs() < 1e-4);
}

#[test]
fn fit_centers_and_contains_the_box() {
    let camera = looking(StandardView::Front);
    let bounds = Aabb {
        min: Vec3::new(10.0, -1.0, 0.0),
        max: Vec3::new(30.0, 1.0, 5.0),
    };
    let pose = camera.fit_pose(bounds, 2.0);
    assert!(close(pose.target, Vec3::new(20.0, 0.0, 2.5)));
    // 20 wide at aspect 2 needs a half height of at least 5.
    assert!(pose.extent >= 5.0 && pose.extent < 6.0);
}

#[test]
fn projection_round_trips_through_rays() {
    for projection in Projection::ALL {
        let camera = Camera {
            projection,
            ..Camera::default()
        };
        let ndc = camera.project(Vec3::new(3.0, 4.0, 5.0), 1.3).unwrap();
        assert!(
            ndc.z >= 0.0 && ndc.z <= 1.0,
            "{projection:?} depth {}",
            ndc.z
        );
        let (origin, dir) = camera.ray(ndc.truncate(), 1.3);
        let to_point = Vec3::new(3.0, 4.0, 5.0) - origin;
        let off_ray = to_point - dir * to_point.dot(dir);
        assert!(off_ray.length() < 1e-3, "{projection:?}");
    }
}

#[test]
fn animation_reaches_its_target() {
    let from = Camera::default().pose();
    let to = Pose {
        target: Vec3::ONE,
        rotation: StandardView::Top.rotation(),
        extent: 5.0,
    };
    let animation = Animation::new(from, to, 0.0, 0.25);
    let (mid, done) = animation.sample(0.125);
    assert!(!done && mid.extent < 20.0 && mid.extent > 5.0);
    let (end, done) = animation.sample(1.0);
    assert!(done && close(end.target, Vec3::ONE));
}
