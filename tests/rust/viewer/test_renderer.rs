use super::*;

/// The scene shaders, parsed and validated by wgpu's own front end.
fn scene_module() -> naga::Module {
    let source = include_str!("../../../src/viewer/shaders.wgsl");
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("invalid WGSL: {error:?}"));
    module
}

/// The size WGSL lays out for the uniform variable `name`.
fn uniform_size(module: &naga::Module, name: &str) -> u32 {
    let mut layouter = naga::proc::Layouter::default();
    layouter.update(module.to_ctx()).unwrap();
    let (_, variable) = module
        .global_variables
        .iter()
        .find(|(_, variable)| variable.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no uniform {name}"));
    layouter[variable.ty].size
}

#[test]
fn the_scene_shaders_are_valid_wgsl_with_every_entry_point() {
    let module = scene_module();
    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for name in [
        "vs_background",
        "fs_background",
        "vs_mesh",
        "fs_mesh",
        "fs_section_count",
        "vs_cap",
        "fs_cap",
        "vs_edge",
        "fs_edge",
        "vs_point",
        "fs_point",
    ] {
        assert!(entry_points.contains(&name), "missing entry point {name}");
    }
}

#[test]
fn uniform_blocks_match_their_wgsl_layout() {
    let module = scene_module();
    assert_eq!(
        uniform_size(&module, "camera") as usize,
        std::mem::size_of::<CameraUniform>()
    );
    assert_eq!(
        uniform_size(&module, "object") as usize,
        std::mem::size_of::<ObjectUniform>()
    );
}

// -- on a GPU ------------------------------------------------------------------

use crate::viewer::camera::StandardView;
use crate::viewer::section::{Section, SectionAxis};

const BACKGROUND: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const BACKGROUND_SRGB: [u8; 4] = [0, 0, 255, 255];

/// Any adapter, software ones included. Without one there is nothing to
/// draw on, and the GPU tests pass vacuously (the shader tests above still
/// check what can be checked).
fn gpu() -> Option<SceneRenderer> {
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()));
    let Ok(adapter) = adapter else {
        eprintln!("no GPU adapter: skipping the rendering test");
        return None;
    };
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(SceneRenderer::with_device(device, queue, &adapter))
}

/// wgpu's native futures are ready once polled; this is all they need.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::yield_now();
    }
}

/// A closed cube with flat faces, centred on the origin.
fn cube(half: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut normal = [0.0; 3];
            normal[axis] = sign;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = positions.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut corner = [0.0; 3];
                corner[axis] = sign * half;
                corner[u] = a * half;
                corner[v] = b * half;
                positions.push(corner);
                normals.push(normal);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(positions, normals, indices, Vec::new(), Vec::new())
}

fn red(clipped: bool) -> Style {
    Style {
        face_color: [1.0, 0.0, 0.0, 1.0],
        edge_color: [0.0, 0.0, 0.0, 1.0],
        highlight: [0.0; 4],
        edge_width: 1.0,
        point_size: 1.0,
        faces: true,
        edges: false,
        points: false,
        clipped,
    }
}

/// Renders a 2-unit cube seen from the top and returns the sRGB pixels at
/// the centre and in a corner, outside the cube.
fn render_cube(
    renderer: &mut SceneRenderer,
    section: Option<Section>,
    clipped: bool,
) -> ([u8; 4], [u8; 4]) {
    let mesh = Arc::new(cube(1.0));
    let bounds = mesh.bounds;
    let mut camera = Camera {
        scene_radius: 2.0,
        ..Camera::default()
    };
    camera.set_pose(super::super::camera::Pose {
        target: Vec3::ZERO,
        rotation: StandardView::Top.rotation(),
        extent: 4.0,
    });
    let items = [DrawItem {
        mesh,
        transform: Mat4::IDENTITY,
        style: red(clipped),
        layer: Layer::Scene,
    }];
    let (size, pixels) = renderer
        .render_pixels(&FrameInput {
            camera: &camera,
            size: [64, 64],
            background_top: BACKGROUND,
            background_bottom: BACKGROUND,
            items: &items,
            section: section.and_then(|section| section.renderer_plane(bounds)),
        })
        .expect("the frame is read back");
    assert_eq!(size, [64, 64]);
    let pixel = |x: u32, y: u32| {
        let start = ((y * size[0] + x) * 4) as usize;
        <[u8; 4]>::try_from(&pixels[start..start + 4]).unwrap()
    };
    (pixel(32, 32), pixel(2, 2))
}

fn cut(offset: f32, caps: bool) -> Option<Section> {
    Some(Section {
        enabled: true,
        axis: SectionAxis::Z,
        offset,
        flipped: false,
        caps,
    })
}

#[test]
fn a_section_plane_cuts_the_model_and_fills_the_cut_on_the_gpu() {
    let Some(mut renderer) = gpu() else { return };

    let (whole, outside) = render_cube(&mut renderer, None, true);
    assert_eq!(outside, BACKGROUND_SRGB);
    assert!(
        whole[0] > 100 && whole[2] < 50,
        "the red top face: {whole:?}"
    );

    // Cut through the middle: the cap, flat and darker than a face, covers
    // the opening, and the stencil leaves nothing outside the cube.
    let (capped, outside) = render_cube(&mut renderer, cut(0.0, true), true);
    assert_eq!(outside, BACKGROUND_SRGB);
    assert!(capped[0] > 50 && capped[2] < 50, "a red cap: {capped:?}");
    assert!(capped[0] < whole[0], "the cap is darker than the top face");

    // Without caps the inside of the bottom face shows through instead.
    let (open, _) = render_cube(&mut renderer, cut(0.0, false), true);
    assert_ne!(open, capped);

    // A plane under the cube removes all of it, caps included.
    let (gone, _) = render_cube(&mut renderer, cut(-5.0, true), true);
    assert_eq!(gone, BACKGROUND_SRGB);

    // Helpers that are not clipped stay whole.
    let (helper, _) = render_cube(&mut renderer, cut(-5.0, true), false);
    assert_eq!(helper, whole);
}
