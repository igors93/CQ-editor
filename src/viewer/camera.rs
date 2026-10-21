//! Orbit camera for the 3D view.
//!
//! The camera orbits around `target`. `rotation` maps camera space to world
//! space; the camera looks down its local -Z axis with +Y up. Zoom is
//! expressed as `extent`, the half height of the visible area at the target,
//! which keeps the framing identical when switching between orthographic and
//! perspective projection.

use glam::camera::rh::{proj::directx, view};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use crate::scene::Aabb;
use crate::settings::Projection;

/// Radians of rotation per pixel of mouse motion (turntable).
const ROTATE_SPEED: f32 = 0.008;
const MIN_EXTENT: f32 = 1e-4;
const MAX_EXTENT: f32 = 1e7;
/// Empty space left around the model by "fit".
const FIT_MARGIN: f32 = 1.08;

/// The fixed views of the View menu and toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardView {
    Iso,
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
}

impl StandardView {
    pub const ALL: [StandardView; 7] = [
        StandardView::Iso,
        StandardView::Top,
        StandardView::Bottom,
        StandardView::Front,
        StandardView::Back,
        StandardView::Left,
        StandardView::Right,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StandardView::Iso => "Iso",
            StandardView::Top => "Top",
            StandardView::Bottom => "Bottom",
            StandardView::Front => "Front",
            StandardView::Back => "Back",
            StandardView::Left => "Left",
            StandardView::Right => "Right",
        }
    }

    /// Direction from the target towards the eye, and the up vector.
    pub fn orientation(self) -> (Vec3, Vec3) {
        match self {
            StandardView::Iso => (Vec3::new(1.0, -1.0, 1.0), Vec3::Z),
            StandardView::Top => (Vec3::Z, Vec3::Y),
            StandardView::Bottom => (Vec3::NEG_Z, Vec3::NEG_Y),
            StandardView::Front => (Vec3::NEG_Y, Vec3::Z),
            StandardView::Back => (Vec3::Y, Vec3::Z),
            StandardView::Left => (Vec3::NEG_X, Vec3::Z),
            StandardView::Right => (Vec3::X, Vec3::Z),
        }
    }

    pub fn rotation(self) -> Quat {
        let (eye, up) = self.orientation();
        look_rotation(eye, up)
    }
}

/// Rotation of a camera whose eye lies in direction `eye_dir` from the
/// target, with `up` projected to the screen's vertical.
pub fn look_rotation(eye_dir: Vec3, up: Vec3) -> Quat {
    let back = eye_dir.normalize();
    let mut right = up.cross(back);
    if right.length_squared() < 1e-10 {
        // `up` is parallel to the view direction: pick any perpendicular.
        right = Vec3::X.cross(back);
        if right.length_squared() < 1e-10 {
            right = Vec3::Y.cross(back);
        }
    }
    let right = right.normalize();
    Quat::from_mat3(&Mat3::from_cols(right, back.cross(right), back)).normalize()
}

/// The animatable part of the camera state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub target: Vec3,
    pub rotation: Quat,
    pub extent: f32,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub target: Vec3,
    pub rotation: Quat,
    pub extent: f32,
    pub projection: Projection,
    /// Vertical field of view of the perspective projection, radians.
    pub fov_y: f32,
    /// Bounding sphere of the scene, used to place the clipping planes.
    pub scene_center: Vec3,
    pub scene_radius: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            rotation: StandardView::Iso.rotation(),
            extent: 20.0,
            projection: Projection::Orthographic,
            fov_y: 30f32.to_radians(),
            scene_center: Vec3::ZERO,
            scene_radius: 20.0,
        }
    }
}

impl Camera {
    pub fn pose(&self) -> Pose {
        Pose {
            target: self.target,
            rotation: self.rotation,
            extent: self.extent,
        }
    }

    pub fn set_pose(&mut self, pose: Pose) {
        self.target = pose.target;
        self.rotation = pose.rotation.normalize();
        self.extent = pose.extent.clamp(MIN_EXTENT, MAX_EXTENT);
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::NEG_Z
    }

    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    pub fn is_perspective(&self) -> bool {
        self.projection == Projection::Perspective
    }

    /// Distance from the eye to the target; for orthographic projection it
    /// only matters for the clipping planes.
    pub fn distance(&self) -> f32 {
        self.extent / (self.fov_y * 0.5).tan()
    }

    pub fn eye(&self) -> Vec3 {
        self.target - self.forward() * self.distance()
    }

    pub fn view_matrix(&self) -> Mat4 {
        view::look_to_mat4(self.eye(), self.forward(), self.up())
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let (near, far) = self.clip_planes();
        match self.projection {
            // WebGPU clip space: depth in 0..1, y up.
            Projection::Perspective => directx::perspective(self.fov_y, aspect, near, far),
            Projection::Orthographic => {
                let (h, w) = (self.extent, self.extent * aspect);
                directx::orthographic(-w, w, -h, h, near, far)
            }
        }
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// Near and far planes enclosing the scene's bounding sphere.
    fn clip_planes(&self) -> (f32, f32) {
        let distance = self.distance();
        let center_depth = (self.scene_center - self.eye()).dot(self.forward());
        let radius = self.scene_radius.max(self.extent).max(1e-3) * 1.5;
        match self.projection {
            Projection::Perspective => {
                let near = (center_depth - radius).max(distance * 1e-3);
                let far = (center_depth + radius).max(near * 2.0);
                (near, far)
            }
            Projection::Orthographic => {
                // Negative near planes are fine for orthographic projection
                // and keep geometry behind the eye position visible.
                let near = center_depth.min(distance) - radius;
                let far = center_depth.max(distance) + radius;
                (near, far)
            }
        }
    }

    /// World-space ray through a point given in normalized device
    /// coordinates (x right, y up, both in -1..1).
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let (right, up, forward) = (self.right(), self.up(), self.forward());
        match self.projection {
            Projection::Perspective => {
                let tan = (self.fov_y * 0.5).tan();
                let dir = forward + right * (ndc.x * tan * aspect) + up * (ndc.y * tan);
                (self.eye(), dir.normalize())
            }
            Projection::Orthographic => {
                let on_target = self.target
                    + right * (ndc.x * self.extent * aspect)
                    + up * (ndc.y * self.extent);
                let back_off = self.distance() + 2.0 * self.scene_radius + self.extent;
                (on_target - forward * back_off, forward)
            }
        }
    }

    /// Turntable orbit: horizontal motion spins around the world Z axis,
    /// vertical motion tilts around the screen's horizontal axis.
    pub fn orbit_turntable(&mut self, delta_px: Vec2) {
        let yaw = Quat::from_rotation_z(-delta_px.x * ROTATE_SPEED);
        let pitch = Quat::from_axis_angle(self.right(), -delta_px.y * ROTATE_SPEED);
        self.rotation = (yaw * pitch * self.rotation).normalize();
    }

    /// Virtual trackball between two pointer positions given in coordinates
    /// normalized by the viewport (y up, roughly -1..1).
    pub fn orbit_trackball(&mut self, from: Vec2, to: Vec2) {
        let (a, b) = (trackball_point(from), trackball_point(to));
        let axis = a.cross(b);
        if axis.length_squared() < 1e-12 {
            return;
        }
        let angle = a.dot(b).clamp(-1.0, 1.0).acos() * 2.0;
        // The model turns with the pointer, so the camera turns the other way.
        let model_turn = Quat::from_axis_angle(axis.normalize(), angle);
        self.rotation = (self.rotation * model_turn.inverse()).normalize();
    }

    /// Removes any roll so that world Z points up on screen (used when
    /// switching to turntable mode).
    pub fn level(&mut self) {
        let back = -self.forward();
        let up = if back.cross(Vec3::Z).length_squared() < 1e-8 {
            self.up()
        } else {
            Vec3::Z
        };
        self.rotation = look_rotation(back, up);
    }

    /// Moves the view so the scene follows the pointer.
    pub fn pan(&mut self, delta_px: Vec2, viewport_height_px: f32) {
        let units_per_px = 2.0 * self.extent / viewport_height_px.max(1.0);
        self.target += (-self.right() * delta_px.x + self.up() * delta_px.y) * units_per_px;
    }

    /// Scales the view by `factor` (< 1 zooms in), keeping the point under
    /// `anchor_ndc` fixed on screen.
    pub fn zoom(&mut self, factor: f32, anchor_ndc: Vec2, aspect: f32) {
        let new_extent = (self.extent * factor).clamp(MIN_EXTENT, MAX_EXTENT);
        let factor = new_extent / self.extent;
        let anchor = self.target
            + self.right() * (anchor_ndc.x * self.extent * aspect)
            + self.up() * (anchor_ndc.y * self.extent);
        self.target = anchor + (self.target - anchor) * factor;
        self.extent = new_extent;
    }

    /// The pose that frames `bounds` with the current orientation.
    pub fn fit_pose(&self, bounds: Aabb, aspect: f32) -> Pose {
        let bounds = if bounds.is_empty() {
            Aabb {
                min: Vec3::splat(-10.0),
                max: Vec3::splat(10.0),
            }
        } else {
            bounds
        };
        let center = bounds.center();
        let (right, up) = (self.right(), self.up());
        let (mut half_width, mut half_height) = (0f32, 0f32);
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 {
                    bounds.min.x
                } else {
                    bounds.max.x
                },
                if i & 2 == 0 {
                    bounds.min.y
                } else {
                    bounds.max.y
                },
                if i & 4 == 0 {
                    bounds.min.z
                } else {
                    bounds.max.z
                },
            ) - center;
            half_width = half_width.max(corner.dot(right).abs());
            half_height = half_height.max(corner.dot(up).abs());
        }
        let mut extent = half_height.max(half_width / aspect.max(1e-3)) * FIT_MARGIN;
        if self.is_perspective() {
            // Parts in front of the target plane appear larger.
            extent *= 1.15;
        }
        Pose {
            target: center,
            rotation: self.rotation,
            extent: extent.max(MIN_EXTENT * 10.0),
        }
    }

    /// Projects a world point to normalized device coordinates; `None`
    /// behind the eye.
    pub fn project(&self, point: Vec3, aspect: f32) -> Option<Vec3> {
        let clip = self.view_projection(aspect) * point.extend(1.0);
        (clip.w > 1e-6).then(|| clip.truncate() / clip.w)
    }
}

/// Holroyd's trackball: a sphere blended into a hyperbolic sheet, so the
/// rotation stays smooth away from the center.
fn trackball_point(p: Vec2) -> Vec3 {
    let d2 = p.length_squared();
    if d2 <= 0.5 {
        Vec3::new(p.x, p.y, (1.0 - d2).sqrt())
    } else {
        Vec3::new(p.x, p.y, 0.5 / d2.sqrt()).normalize()
    }
}

/// Smooth transition between two poses.
#[derive(Clone, Debug)]
pub struct Animation {
    from: Pose,
    to: Pose,
    start: f64,
    duration: f64,
}

impl Animation {
    pub fn new(from: Pose, to: Pose, now: f64, duration: f64) -> Self {
        Self {
            from,
            to,
            start: now,
            duration,
        }
    }

    pub fn target(&self) -> Pose {
        self.to
    }

    /// The pose at time `now`, and whether the animation is over.
    pub fn sample(&self, now: f64) -> (Pose, bool) {
        let t = if self.duration > 0.0 {
            ((now - self.start) / self.duration).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        // Cubic ease in-out.
        let e = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        };
        let pose = Pose {
            target: self.from.target.lerp(self.to.target, e),
            rotation: self.from.rotation.slerp(self.to.rotation, e),
            extent: (self.from.extent.ln() + (self.to.extent.ln() - self.from.extent.ln()) * e)
                .exp(),
        };
        (pose, t >= 1.0)
    }
}

#[cfg(test)]
mod tests {
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
        let after_on_plane =
            after + camera.forward() * (camera.target - after).dot(camera.forward());
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
}
