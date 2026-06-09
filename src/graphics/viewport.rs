use crate::mesh::Vertex;
use glam::{Mat4, Vec3};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,   // in radians
    pub pitch: f32, // in radians
    pub distance: f32,
    pub aspect: f32,
    pub fovy: f32, // in radians
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            target: Vec3::new(0.0, 0.0, 0.0),
            yaw: std::f32::consts::FRAC_PI_4, // 45 degrees
            pitch: 0.25,                      // slightly looking down
            distance: 4.5,
            aspect,
            fovy: std::f32::consts::FRAC_PI_4, // 45 degrees fov
            znear: 0.1,
            zfar: 100.0,
        }
    }

    pub fn get_eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.cos() + self.target.x;
        let y = self.distance * self.pitch.sin() + self.target.y;
        let z = self.distance * self.pitch.cos() * self.yaw.sin() + self.target.z;
        Vec3::new(x, y, z)
    }

    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let eye = self.get_eye();
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewTransform {
    Standard,
    AgX,
    ACES,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view_position: [f32; 4],
    pub light_dir: [f32; 4],
    pub light_color: [f32; 4],
    pub ambient_strength: f32,
    pub view_transform: f32, // 0.0 for Standard, 1.0 for AgX, 2.0 for ACES
    pub exposure: f32,
    pub num_udim_tiles: f32,
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view_position: [0.0, 0.0, 0.0, 1.0],
            light_dir: [-1.0, -1.0, -0.5, 0.0],
            light_color: [1.0, 1.0, 1.0, 1.0],
            ambient_strength: 0.25,
            view_transform: 0.0,
            exposure: 1.0,
            num_udim_tiles: 1.0,
        }
    }

    pub fn update_view_proj(
        &mut self,
        camera: &crate::app::ecs::CameraResource,
        light_angle: f32,
        light_intensity: f32,
        ambient_strength: f32,
        view_transform: ViewTransform,
        exposure: f32,
        num_udim_tiles: u32,
    ) {
        self.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();
        let eye = camera.get_eye();
        self.view_position = [eye.x, eye.y, eye.z, 1.0];

        // Rotate light around Y axis
        let lx = light_angle.cos();
        let lz = light_angle.sin();
        self.light_dir = [lx, -1.0, lz, 0.0];
        self.light_color[3] = light_intensity;

        self.ambient_strength = ambient_strength;
        self.view_transform = match view_transform {
            ViewTransform::Standard => 0.0,
            ViewTransform::AgX => 1.0,
            ViewTransform::ACES => 2.0,
        };
        self.exposure = exposure;
        self.num_udim_tiles = num_udim_tiles as f32;
    }
}

pub struct Viewport {
    pub node_bind_group_layout: Arc<wgpu::BindGroupLayout>,
    pub render_pipeline: wgpu::RenderPipeline,
}

impl Viewport {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        aspect: f32,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> (
        Self,
        crate::app::ecs::CameraResource,
        wgpu::Buffer,
        wgpu::BindGroup,
        crate::mesh::Document,
    ) {
        // Setup camera and uniforms
        let camera = Camera::new(aspect);
        let camera_res = crate::app::ecs::CameraResource {
            target: camera.target,
            yaw: camera.yaw,
            pitch: camera.pitch,
            distance: camera.distance,
            aspect: camera.aspect,
            fovy: camera.fovy,
            znear: camera.znear,
            zfar: camera.zfar,
        };

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera_res, 0.0, 1.0, 0.25, ViewTransform::Standard, 1.0, 1);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Uniform Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Camera Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        // Setup node uniform bind group layout
        let node_bind_group_layout = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Node Bind Group Layout"),
            },
        ));

        // Generate procedural sphere document
        let document =
            crate::mesh::create_sphere_document(device, &node_bind_group_layout, 1.5, 32, 32);

        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "../shaders/3d_mesh.wgsl"
            ))),
        });

        // Pipeline layout includes Group 0 (Camera), Group 1 (Texture), Group 2 (Node)
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("3D Mesh Render Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&camera_bind_group_layout),
                    Some(texture_bind_group_layout),
                    Some(&*node_bind_group_layout),
                ],
                immediate_size: 0,
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Mesh Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let vp = Self {
            node_bind_group_layout,
            render_pipeline,
        };
        (vp, camera_res, camera_buffer, camera_bind_group, document)
    }

    pub fn update_node_transforms(&self, world: &mut bevy_ecs::prelude::World, queue: &wgpu::Queue) {
        let mut query = world.query::<(&crate::app::ecs::Transform, &crate::app::ecs::NodeGpuResources)>();
        for (transform, gpu_res) in query.iter(world) {
            let world_matrix = transform.to_matrix();
            let normal_matrix = world_matrix.inverse().transpose();
            let uniform = crate::mesh::NodeUniform {
                model_matrix: world_matrix.to_cols_array_2d(),
                normal_matrix: normal_matrix.to_cols_array_2d(),
            };
            queue.write_buffer(&gpu_res.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
        }
    }

    pub fn render(
        &self,
        world: &mut bevy_ecs::prelude::World,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        texture_bind_group: &wgpu::BindGroup,
    ) {
        let mut query = world.query::<(&crate::app::ecs::MeshHandle, &crate::app::ecs::NodeGpuResources)>();
        let nodes: Vec<(std::sync::Arc<crate::mesh::Mesh>, std::sync::Arc<wgpu::BindGroup>)> = query
            .iter(world)
            .map(|(handle, gpu_res)| (handle.0.clone(), gpu_res.bind_group.clone()))
            .collect();

        let camera_gpu = world.get_resource::<crate::app::ecs::CameraGpuResources>().expect("Camera GPU Resources");
        let main_ctx = world.get_resource::<crate::app::ecs::MainRenderContextResource>().expect("Main Render Context");

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D Viewport Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.08,
                        g: 0.08,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &main_ctx.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &camera_gpu.bind_group, &[]);
        render_pass.set_bind_group(1, texture_bind_group, &[]);

        for (mesh_handle, gpu_res) in &nodes {
            render_pass.set_bind_group(2, &**gpu_res, &[]);
            for primitive in &mesh_handle.primitives {
                render_pass.set_vertex_buffer(0, primitive.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    primitive.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..primitive.num_indices, 0, 0..1);
            }
        }
    }
}

pub fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: config.width,
        height: config.height,
        depth_or_array_layers: 1,
    };
    let desc = wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    };
    let texture = device.create_texture(&desc);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
