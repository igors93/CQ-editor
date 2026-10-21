//! GPU renderer of the 3D view.
//!
//! The scene is drawn off screen (4x MSAA when available) into an sRGB
//! texture that egui then shows like an image. egui samples textures as
//! gamma-encoded values, so it is handed a non-sRGB view of the same texture.
//!
//! Draw order: background, opaque faces, edges and points, transparent faces
//! (back to front), then the inspector overlay, which is drawn on top.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::{Arc, Weak};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::camera::Camera;
use crate::scene::Mesh;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Same texels as `COLOR_FORMAT`, read without sRGB decoding (for egui).
const EGUI_VIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const PREFERRED_SAMPLES: u32 = 4;
const OBJECT_UNIFORM_SIZE: u64 = std::mem::size_of::<ObjectUniform>() as u64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    forward: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    viewport: [f32; 4],
    background_top: [f32; 4],
    background_bottom: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniform {
    model: [[f32; 4]; 4],
    face_color: [f32; 4],
    edge_color: [f32; 4],
    highlight: [f32; 4],
    params: [f32; 4],
}

/// How one mesh is drawn. Colors are linear RGBA.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub face_color: [f32; 4],
    pub edge_color: [f32; 4],
    /// RGB tint mixed into faces, with the mix factor in alpha.
    pub highlight: [f32; 4],
    pub edge_width: f32,
    pub point_size: f32,
    pub faces: bool,
    pub edges: bool,
    pub points: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Scene,
    /// Drawn after the scene, faces pulled towards the viewer and edges and
    /// points always visible.
    Overlay,
}

pub struct DrawItem {
    pub mesh: Arc<Mesh>,
    pub transform: Mat4,
    pub style: Style,
    pub layer: Layer,
}

/// Everything needed to draw one frame.
pub struct FrameInput<'a> {
    pub camera: &'a Camera,
    /// Physical pixels.
    pub size: [u32; 2],
    /// Linear RGBA.
    pub background_top: [f32; 4],
    pub background_bottom: [f32; 4],
    pub items: &'a [DrawItem],
}

struct GpuMesh {
    source: Weak<Mesh>,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    index_count: u32,
    segments: Option<wgpu::Buffer>,
    segment_count: u32,
    points: Option<wgpu::Buffer>,
    point_count: u32,
}

struct Target {
    size: [u32; 2],
    multisampled: Option<wgpu::TextureView>,
    resolved: wgpu::Texture,
    resolved_view: wgpu::TextureView,
    egui_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

struct Pipelines {
    background: wgpu::RenderPipeline,
    faces: wgpu::RenderPipeline,
    transparent_faces: wgpu::RenderPipeline,
    overlay_faces: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    overlay_edges: wgpu::RenderPipeline,
    points: wgpu::RenderPipeline,
    overlay_points: wgpu::RenderPipeline,
}

pub struct SceneRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    samples: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    object_layout: wgpu::BindGroupLayout,
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,
    object_capacity: usize,
    object_stride: u64,
    pipelines: Pipelines,
    meshes: HashMap<u64, GpuMesh>,
    target: Option<Target>,
    texture_id: Option<egui::TextureId>,
}

impl SceneRenderer {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();

        let format_features = render_state
            .adapter
            .get_texture_format_features(COLOR_FORMAT);
        let samples = if format_features
            .flags
            .sample_count_supported(PREFERRED_SAMPLES)
        {
            PREFERRED_SAMPLES
        } else {
            1
        };

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders.wgsl"));

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let object_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("object"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(OBJECT_UNIFORM_SIZE),
                },
                count: None,
            }],
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let object_stride = OBJECT_UNIFORM_SIZE.div_ceil(alignment) * alignment;
        let object_capacity = 64;
        let (object_buffer, object_bind_group) =
            create_object_buffer(&device, &object_layout, object_stride, object_capacity);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&camera_layout), Some(&object_layout)],
            immediate_size: 0,
        });
        let pipelines = Pipelines::new(&device, &layout, &shader, samples);

        Self {
            device,
            queue,
            samples,
            camera_buffer,
            camera_bind_group,
            object_layout,
            object_buffer,
            object_bind_group,
            object_capacity,
            object_stride,
            pipelines,
            meshes: HashMap::new(),
            target: None,
            texture_id: None,
        }
    }

    /// Draws a frame and returns the egui texture showing it.
    pub fn render(
        &mut self,
        egui_renderer: &mut egui_wgpu::Renderer,
        input: &FrameInput<'_>,
    ) -> egui::TextureId {
        let size = [input.size[0].max(1), input.size[1].max(1)];
        let texture_id = self.ensure_target(egui_renderer, size);

        self.meshes.retain(|_, mesh| mesh.source.strong_count() > 0);
        let uploading = Instant::now();
        let (mut uploaded, mut bytes) = (0, 0);
        for item in input.items {
            if !self.meshes.contains_key(&item.mesh.id) {
                let gpu = upload(&self.device, &item.mesh);
                self.meshes.insert(item.mesh.id, gpu);
                uploaded += 1;
                bytes += item.mesh.byte_size();
            }
        }
        if uploaded > 0 {
            log::debug!(
                "Uploaded {uploaded} mesh(es) to the GPU ({:.2} MB of mesh data) in {:.1} ms",
                bytes as f64 / 1e6,
                uploading.elapsed().as_secs_f64() * 1e3
            );
        }

        self.write_camera(input, size);
        self.write_objects(input.items);

        let target = self.target.as_ref().expect("target created above");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene"),
            });
        {
            let (view, resolve_target) = match &target.multisampled {
                Some(multisampled) => (multisampled, Some(&target.resolved_view)),
                None => (&target.resolved_view, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            pass.set_pipeline(&self.pipelines.background);
            pass.set_bind_group(1, &self.object_bind_group, &[0]);
            pass.draw(0..3, 0..1);

            let is_opaque = |item: &DrawItem| item.style.face_color[3] >= 0.999;
            let order = draw_order(input);
            let scene = |pass_layer: Layer| {
                order
                    .iter()
                    .copied()
                    .filter(move |&(_, layer)| layer == pass_layer)
            };

            for (index, _) in scene(Layer::Scene).filter(|&(i, _)| is_opaque(&input.items[i])) {
                self.draw_faces(&mut pass, &self.pipelines.faces, input, index);
            }
            for (index, _) in scene(Layer::Scene) {
                self.draw_edges(&mut pass, &self.pipelines.edges, input, index);
                self.draw_points(&mut pass, &self.pipelines.points, input, index);
            }
            for (index, _) in scene(Layer::Scene).filter(|&(i, _)| !is_opaque(&input.items[i])) {
                self.draw_faces(&mut pass, &self.pipelines.transparent_faces, input, index);
            }
            for (index, _) in scene(Layer::Overlay) {
                self.draw_faces(&mut pass, &self.pipelines.overlay_faces, input, index);
                self.draw_edges(&mut pass, &self.pipelines.overlay_edges, input, index);
                self.draw_points(&mut pass, &self.pipelines.overlay_points, input, index);
            }
        }
        self.queue.submit([encoder.finish()]);
        texture_id
    }

    /// Reads back the last rendered frame as tightly packed sRGB RGBA rows.
    pub fn read_pixels(&self) -> Option<([u32; 2], Vec<u8>)> {
        let target = self.target.as_ref()?;
        let [width, height] = target.size;
        let row_bytes = width * 4;
        let padded_row = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("screenshot"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.resolved,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .ok()?;
        receiver.recv().ok()?.ok()?;

        let mapped = slice.get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((row_bytes * height) as usize);
        for row in mapped.chunks(padded_row as usize) {
            pixels.extend_from_slice(&row[..row_bytes as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Some(([width, height], pixels))
    }

    fn ensure_target(
        &mut self,
        egui_renderer: &mut egui_wgpu::Renderer,
        size: [u32; 2],
    ) -> egui::TextureId {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let target = create_target(&self.device, size, self.samples);
            match self.texture_id {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    &self.device,
                    &target.egui_view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
                None => {
                    self.texture_id = Some(egui_renderer.register_native_texture(
                        &self.device,
                        &target.egui_view,
                        wgpu::FilterMode::Linear,
                    ));
                }
            }
            self.target = Some(target);
        }
        self.texture_id.expect("texture registered")
    }

    fn write_camera(&self, input: &FrameInput<'_>, size: [u32; 2]) {
        let camera = input.camera;
        let (width, height) = (size[0] as f32, size[1] as f32);
        let vec4 = |v: Vec3, w: f32| [v.x, v.y, v.z, w];
        let uniform = CameraUniform {
            view_proj: camera.view_projection(width / height).to_cols_array_2d(),
            eye: vec4(
                camera.eye(),
                if camera.is_perspective() { 1.0 } else { 0.0 },
            ),
            forward: vec4(camera.forward(), 0.0),
            right: vec4(camera.right(), 0.0),
            up: vec4(camera.up(), 0.0),
            viewport: [width, height, 1.0 / width, 1.0 / height],
            background_top: input.background_top,
            background_bottom: input.background_bottom,
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// One uniform block per item, at `object_stride` intervals; slot 0 is
    /// reserved for the background pass, so item `i` lives in slot `i + 1`.
    fn write_objects(&mut self, items: &[DrawItem]) {
        let needed = items.len() + 1;
        if needed > self.object_capacity {
            self.object_capacity = needed.next_power_of_two();
            let (buffer, bind_group) = create_object_buffer(
                &self.device,
                &self.object_layout,
                self.object_stride,
                self.object_capacity,
            );
            self.object_buffer = buffer;
            self.object_bind_group = bind_group;
        }
        let stride = self.object_stride as usize;
        let mut data = vec![0u8; needed * stride];
        for (slot, item) in items.iter().enumerate() {
            let style = &item.style;
            let uniform = ObjectUniform {
                model: item.transform.to_cols_array_2d(),
                face_color: style.face_color,
                edge_color: style.edge_color,
                highlight: style.highlight,
                params: [style.edge_width, style.point_size, 0.0, 0.0],
            };
            let offset = (slot + 1) * stride;
            data[offset..offset + OBJECT_UNIFORM_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        self.queue.write_buffer(&self.object_buffer, 0, &data);
    }

    fn object_offset(&self, index: usize) -> u32 {
        ((index + 1) as u64 * self.object_stride) as u32
    }

    fn draw_faces(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        input: &FrameInput<'_>,
        index: usize,
    ) {
        let item = &input.items[index];
        let Some(mesh) = self.meshes.get(&item.mesh.id) else {
            return;
        };
        if !item.style.faces || mesh.index_count == 0 {
            return;
        }
        let (Some(vertices), Some(indices)) = (&mesh.vertices, &mesh.indices) else {
            return;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(1, &self.object_bind_group, &[self.object_offset(index)]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }

    fn draw_edges(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        input: &FrameInput<'_>,
        index: usize,
    ) {
        let item = &input.items[index];
        let Some(mesh) = self.meshes.get(&item.mesh.id) else {
            return;
        };
        let Some(segments) = &mesh.segments else {
            return;
        };
        if !item.style.edges || mesh.segment_count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(1, &self.object_bind_group, &[self.object_offset(index)]);
        pass.set_vertex_buffer(0, segments.slice(..));
        pass.draw(0..6, 0..mesh.segment_count);
    }

    fn draw_points(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        input: &FrameInput<'_>,
        index: usize,
    ) {
        let item = &input.items[index];
        let Some(mesh) = self.meshes.get(&item.mesh.id) else {
            return;
        };
        let Some(points) = &mesh.points else { return };
        if !item.style.points || mesh.point_count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(1, &self.object_bind_group, &[self.object_offset(index)]);
        pass.set_vertex_buffer(0, points.slice(..));
        pass.draw(0..6, 0..mesh.point_count);
    }
}

/// Item indices with their layers: opaque order is irrelevant, transparent
/// faces need back-to-front order.
fn draw_order(input: &FrameInput<'_>) -> Vec<(usize, Layer)> {
    let camera = input.camera;
    let eye = camera.eye();
    let forward = camera.forward();
    let mut order: Vec<(usize, f32)> = input
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let center = item.transform.transform_point3(item.mesh.bounds.center());
            (i, (center - eye).dot(forward))
        })
        .collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));
    order
        .into_iter()
        .map(|(i, _)| (i, input.items[i].layer))
        .collect()
}

fn create_object_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    stride: u64,
    capacity: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("objects"),
        size: stride * capacity as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("objects"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: NonZeroU64::new(OBJECT_UNIFORM_SIZE),
            }),
        }],
    });
    (buffer, bind_group)
}

fn create_target(device: &wgpu::Device, size: [u32; 2], samples: u32) -> Target {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let texture = |label, format, samples, usage, view_formats: &[wgpu::TextureFormat]| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats,
        })
    };

    let resolved = texture(
        "scene color",
        COLOR_FORMAT,
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        &[EGUI_VIEW_FORMAT],
    );
    let resolved_view = resolved.create_view(&wgpu::TextureViewDescriptor::default());
    let egui_view = resolved.create_view(&wgpu::TextureViewDescriptor {
        format: Some(EGUI_VIEW_FORMAT),
        ..Default::default()
    });
    let multisampled = (samples > 1).then(|| {
        texture(
            "scene msaa",
            COLOR_FORMAT,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            &[],
        )
        .create_view(&wgpu::TextureViewDescriptor::default())
    });
    let depth_view = texture(
        "scene depth",
        DEPTH_FORMAT,
        samples,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
        &[],
    )
    .create_view(&wgpu::TextureViewDescriptor::default());

    Target {
        size,
        multisampled,
        resolved,
        resolved_view,
        egui_view,
        depth_view,
    }
}

fn upload(device: &wgpu::Device, mesh: &Arc<Mesh>) -> GpuMesh {
    let buffer = |label, contents: &[u8], usage| {
        (!contents.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        })
    };
    let interleaved: Vec<[f32; 6]> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .map(|(p, n)| [p[0], p[1], p[2], n[0], n[1], n[2]])
        .collect();
    let has_triangles = !interleaved.is_empty() && !mesh.indices.is_empty();
    GpuMesh {
        source: Arc::downgrade(mesh),
        vertices: has_triangles
            .then(|| {
                buffer(
                    "vertices",
                    bytemuck::cast_slice(&interleaved),
                    wgpu::BufferUsages::VERTEX,
                )
            })
            .flatten(),
        indices: has_triangles
            .then(|| {
                buffer(
                    "indices",
                    bytemuck::cast_slice(&mesh.indices),
                    wgpu::BufferUsages::INDEX,
                )
            })
            .flatten(),
        index_count: if has_triangles {
            mesh.indices.len() as u32
        } else {
            0
        },
        segments: buffer(
            "segments",
            bytemuck::cast_slice(&mesh.edges),
            wgpu::BufferUsages::VERTEX,
        ),
        segment_count: (mesh.edges.len() / 2) as u32,
        points: buffer(
            "points",
            bytemuck::cast_slice(&mesh.points),
            wgpu::BufferUsages::VERTEX,
        ),
        point_count: mesh.points.len() as u32,
    }
}

impl Pipelines {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        samples: u32,
    ) -> Self {
        const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        const SEGMENT_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        const POINT_ATTRIBUTES: [wgpu::VertexAttribute; 1] =
            wgpu::vertex_attr_array![0 => Float32x3];

        let mesh_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MESH_ATTRIBUTES,
        })];
        let segment_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &SEGMENT_ATTRIBUTES,
        })];
        let point_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &POINT_ATTRIBUTES,
        })];

        let builder = PipelineBuilder {
            device,
            layout,
            shader,
            samples,
        };
        // Faces are pushed slightly back so that edges lying on them win the
        // depth test; overlay faces are pulled forward instead.
        let push_back = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.5,
            clamp: 0.0,
        };
        let pull_forward = wgpu::DepthBiasState {
            constant: -4,
            slope_scale: -2.0,
            clamp: 0.0,
        };
        let no_bias = wgpu::DepthBiasState::default();
        use wgpu::CompareFunction::{Always, Less, LessEqual};

        Self {
            background: builder.build(
                "background",
                "vs_background",
                "fs_background",
                &[],
                None,
                Always,
                false,
                no_bias,
            ),
            faces: builder.build(
                "faces",
                "vs_mesh",
                "fs_mesh",
                &mesh_buffers,
                None,
                Less,
                true,
                push_back,
            ),
            transparent_faces: builder.build(
                "transparent faces",
                "vs_mesh",
                "fs_mesh",
                &mesh_buffers,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                Less,
                false,
                push_back,
            ),
            overlay_faces: builder.build(
                "overlay faces",
                "vs_mesh",
                "fs_mesh",
                &mesh_buffers,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                LessEqual,
                false,
                pull_forward,
            ),
            edges: builder.build(
                "edges",
                "vs_edge",
                "fs_edge",
                &segment_buffers,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                LessEqual,
                true,
                no_bias,
            ),
            overlay_edges: builder.build(
                "overlay edges",
                "vs_edge",
                "fs_edge",
                &segment_buffers,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                Always,
                false,
                no_bias,
            ),
            points: builder.build(
                "points",
                "vs_point",
                "fs_point",
                &point_buffers,
                None,
                LessEqual,
                true,
                no_bias,
            ),
            overlay_points: builder.build(
                "overlay points",
                "vs_point",
                "fs_point",
                &point_buffers,
                None,
                Always,
                false,
                no_bias,
            ),
        }
    }
}

struct PipelineBuilder<'a> {
    device: &'a wgpu::Device,
    layout: &'a wgpu::PipelineLayout,
    shader: &'a wgpu::ShaderModule,
    samples: u32,
}

impl PipelineBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        label: &str,
        vertex_entry: &str,
        fragment_entry: &str,
        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
        blend: Option<wgpu::BlendState>,
        depth_compare: wgpu::CompareFunction,
        depth_write: bool,
        bias: wgpu::DepthBiasState,
    ) -> wgpu::RenderPipeline {
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(self.layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(depth_compare),
                    stencil: wgpu::StencilState::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: blend.or(Some(wgpu::BlendState::REPLACE)),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}
