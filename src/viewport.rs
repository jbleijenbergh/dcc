use wgpu::util::DeviceExt;
use glam::{Mat4, Vec3};
use crate::mesh::{Vertex, create_sphere};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,       // in radians
    pub pitch: f32,     // in radians
    pub distance: f32,
    pub aspect: f32,
    pub fovy: f32,      // in radians
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            target: Vec3::new(0.0, 0.0, 0.0),
            yaw: std::f32::consts::FRAC_PI_4, // 45 degrees
            pitch: 0.25,                     // slightly looking down
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
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    view_position: [f32; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient_strength: f32,
    view_transform: f32, // 0.0 for Standard, 1.0 for AgX, 2.0 for ACES
    exposure: f32,
    padding: f32,
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view_position: [0.0, 0.0, 0.0, 1.0],
            light_dir: [-1.0, -1.0, -0.5, 0.0],
            light_color: [1.0, 1.0, 1.0, 1.0],
            ambient_strength: 0.25,
            view_transform: 0.0,
            exposure: 1.0,
            padding: 0.0,
        }
    }

    fn update_view_proj(
        &mut self,
        camera: &Camera,
        light_angle: f32,
        light_intensity: f32,
        ambient_strength: f32,
        view_transform: ViewTransform,
        exposure: f32,
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
    }
}

pub struct Viewport {
    pub camera: Camera,
    pub light_angle: f32,
    pub light_intensity: f32,
    pub ambient_strength: f32,
    pub view_transform: ViewTransform,
    pub exposure: f32,
    pub mesh_vertices: Vec<Vertex>,
    pub mesh_indices: Vec<u32>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    render_pipeline: wgpu::RenderPipeline,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
}

impl Viewport {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        aspect: f32,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // Generate procedural sphere mesh
        let (mesh_vertices, mesh_indices) = create_sphere(1.5, 32, 32);
        let num_indices = mesh_indices.len() as u32;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sphere Vertex Buffer"),
            contents: bytemuck::cast_slice(&mesh_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sphere Index Buffer"),
            contents: bytemuck::cast_slice(&mesh_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Setup camera and uniforms
        let camera = Camera::new(aspect);
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera, 0.0, 1.0, 0.25, ViewTransform::Standard, 1.0);

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

        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shaders/3d_mesh.wgsl"
            ))),
        });

        // Pipeline layout
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("3D Mesh Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout, texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Mesh Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            camera,
            light_angle: 0.0,
            light_intensity: 1.0,
            ambient_strength: 0.25,
            view_transform: ViewTransform::Standard,
            exposure: 1.0,
            mesh_vertices,
            mesh_indices,
            vertex_buffer,
            index_buffer,
            num_indices,
            render_pipeline,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
        }
    }

    pub fn set_mesh(&mut self, device: &wgpu::Device, vertices: Vec<Vertex>, indices: Vec<u32>) {
        self.num_indices = indices.len() as u32;
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dynamic Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dynamic Mesh Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.mesh_vertices = vertices;
        self.mesh_indices = indices;
    }

    pub fn update_camera(&mut self, queue: &wgpu::Queue) {
        self.camera_uniform.update_view_proj(
            &self.camera,
            self.light_angle,
            self.light_intensity,
            self.ambient_strength,
            self.view_transform,
            self.exposure,
        );
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        texture_bind_group: &wgpu::BindGroup,
    ) {
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
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
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
