use std::borrow::Cow;
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlendMode {
    Normal = 0,
    Multiply = 1,
    Add = 2,
}

impl BlendMode {
    pub fn to_str(&self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Add => "Add",
        }
    }
}

pub struct StrokePoint {
    pub position: glam::Vec3,
    pub uv: glam::Vec2,
}

#[derive(Clone)]
pub struct PaintStroke {
    pub points: Vec<glam::Vec3>,
    pub uv_points: Vec<glam::Vec2>,
    pub color: [u8; 4],
    pub radius: f32,
    pub hardness: f32,
    pub is_eraser: bool,
}

pub struct Layer {
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub blend_mode: BlendMode,
    
    // Fill layer properties
    pub is_fill: bool,
    pub fill_color: [u8; 4],
    pub fill_noise_color: [u8; 4],
    pub fill_noise_scale: f32,
    pub fill_projection_mode: u32, // 0: UV, 1: Triplanar
    
    // Strokes history
    pub strokes: Vec<PaintStroke>,
}

impl Layer {
    pub fn new(name: String) -> Self {
        Self {
            name,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            is_fill: false,
            fill_color: [128, 128, 128, 255],
            fill_noise_color: [255, 255, 255, 255],
            fill_noise_scale: 10.0,
            fill_projection_mode: 0,
            strokes: Vec::new(),
        }
    }

    pub fn new_fill(name: String) -> Self {
        Self {
            name,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            is_fill: true,
            fill_color: [80, 100, 140, 255], // steel blue
            fill_noise_color: [150, 180, 220, 255],
            fill_noise_scale: 15.0,
            fill_projection_mode: 1, // triplanar by default
            strokes: Vec::new(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayerInfo {
    pub opacity: f32,
    pub blend_mode: u32,
    pub visible: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayersUniform {
    pub count: u32,
    pub udim_tile: u32,
    pub padding2: u32,
    pub padding3: u32,
    pub layers: [LayerInfo; 16],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StampInstance {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub radius: f32,
    pub hardness: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BrushUniforms {
    pub resolution: [f32; 2],
    pub padding: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FillUniforms {
    pub base_color: [f32; 4],
    pub noise_color: [f32; 4],
    pub noise_scale: f32,
    pub projection_mode: u32,
    pub udim_tile: u32,
    pub padding: u32,
}

pub const MAX_LAYERS: usize = 8; // Max 8 layers with 4 UDIMs each = 32 views
pub const MAX_UDIMS: usize = 4;

#[allow(dead_code)]
pub struct Painter {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer_idx: usize,
    
    // WGPU Resources
    pub layer_array_texture: wgpu::Texture,
    pub layer_views: Vec<wgpu::TextureView>,
    
    pub texture: wgpu::Texture, // composite texture array
    pub composite_views: Vec<wgpu::TextureView>,
    pub texture_view: wgpu::TextureView, // full array view
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    
    brush_pipeline_add: wgpu::RenderPipeline,
    brush_pipeline_sub: wgpu::RenderPipeline,
    brush_uniform_buffer: wgpu::Buffer,
    brush_bind_group: wgpu::BindGroup,
    
    composite_pipeline: wgpu::RenderPipeline,
    composite_uniform_buffer: wgpu::Buffer,
    composite_bind_group: wgpu::BindGroup,
    
    // Fill Layer resources
    pub fill_pipeline: wgpu::RenderPipeline,
    pub fill_uniform_buffer: wgpu::Buffer,
    pub fill_bind_group_layout: wgpu::BindGroupLayout,
}

impl Painter {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let width = 1024;
        let height = 1024;
        
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: MAX_UDIMS as u32, // 4 UDIM tiles
        };
        let array_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: (MAX_LAYERS * MAX_UDIMS) as u32, // 32 array layers
        };

        // 1. Create textures
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Painted Canvas Texture (Composite)"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        
        let mut composite_views = Vec::new();
        for i in 0..MAX_UDIMS {
            composite_views.push(texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Composite Tile {} View", i)),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            }));
        }
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Composite Array Full View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let layer_array_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Layer Array Texture"),
            size: array_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        
        let mut layer_views = Vec::new();
        for i in 0..MAX_LAYERS * MAX_UDIMS {
            layer_views.push(layer_array_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Layer View {}", i)),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            }));
        }
        let layer_array_view = layer_array_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Layer Array Full View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
            label: Some("Painter Texture Bind Group"),
        });

        // 2. Brush Pipeline
        let brush_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Brush Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/brush.wgsl"))),
        });

        let brush_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Brush Uniform Buffer"),
            contents: bytemuck::cast_slice(&[BrushUniforms { resolution: [width as f32, height as f32], padding: [0.0, 0.0] }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let brush_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Brush Bind Group Layout"),
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
        });

        let brush_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Brush Bind Group"),
            layout: &brush_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: brush_uniform_buffer.as_entire_binding(),
            }],
        });

        let brush_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Brush Pipeline Layout"),
            bind_group_layouts: &[Some(&brush_bind_group_layout)],
            immediate_size: 0,
        });

        let instance_desc = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<StampInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 }, // position
                wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x4 }, // color
                wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32 }, // radius
                wgpu::VertexAttribute { offset: 28, shader_location: 3, format: wgpu::VertexFormat::Float32 }, // hardness
            ],
        };

        let brush_pipeline_add = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Brush Pipeline Add"),
            layout: Some(&brush_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &brush_shader,
                entry_point: Some("vs_main"),
                buffers: &[instance_desc.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &brush_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let brush_pipeline_sub = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Brush Pipeline Sub"),
            layout: Some(&brush_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &brush_shader,
                entry_point: Some("vs_main"),
                buffers: &[instance_desc.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &brush_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 3. Composite Pipeline
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/composite.wgsl"))),
        });

        let composite_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composite Uniform Buffer"),
            size: std::mem::size_of::<LayersUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let composite_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: composite_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&layer_array_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[Some(&composite_bind_group_layout)],
            immediate_size: 0,
        });

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 4. Fill Pipeline
        let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fill Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/fill.wgsl"))),
        });

        let fill_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Uniform Buffer"),
            size: std::mem::size_of::<FillUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let fill_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fill Bind Group Layout"),
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
        });

        let node_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            label: Some("Painter Node Bind Group Layout"),
        });

        let fill_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fill Pipeline Layout"),
            bind_group_layouts: &[Some(&fill_bind_group_layout), Some(&node_bind_group_layout)],
            immediate_size: 0,
        });

        let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fill Render Pipeline"),
            layout: Some(&fill_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &fill_shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::mesh::Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fill_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None, // Render both sides in UV space
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let default_layer = Layer::new("Layer 1".to_string());

        Self {
            width,
            height,
            layers: vec![default_layer],
            active_layer_idx: 0,
            
            layer_array_texture,
            layer_views,
            
            texture,
            composite_views,
            texture_view,
            sampler,
            bind_group,
            
            brush_pipeline_add,
            brush_pipeline_sub,
            brush_uniform_buffer,
            brush_bind_group,
            
            composite_pipeline,
            composite_uniform_buffer,
            composite_bind_group,
            
            fill_pipeline,
            fill_uniform_buffer,
            fill_bind_group_layout,
        }
    }

    pub fn compose_layers(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for tile_idx in 0..MAX_UDIMS {
            let mut uniform_data = LayersUniform {
                count: self.layers.len() as u32,
                udim_tile: tile_idx as u32,
                padding2: 0,
                padding3: 0,
                layers: [LayerInfo { opacity: 0.0, blend_mode: 0, visible: 0, padding: 0 }; 16],
            };
            for (i, layer) in self.layers.iter().enumerate() {
                uniform_data.layers[i] = LayerInfo {
                    opacity: layer.opacity,
                    blend_mode: layer.blend_mode as u32,
                    visible: if layer.visible { 1 } else { 0 },
                    padding: 0,
                };
            }
            queue.write_buffer(&self.composite_uniform_buffer, 0, bytemuck::cast_slice(&[uniform_data]));

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(&format!("Composite Encoder Tile {}", tile_idx)),
            });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("Composite Pass Tile {}", tile_idx)),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.composite_views[tile_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 230.0 / 255.0,
                                g: 230.0 / 255.0,
                                b: 230.0 / 255.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                rpass.set_pipeline(&self.composite_pipeline);
                rpass.set_bind_group(0, &self.composite_bind_group, &[]);
                rpass.draw(0..3, 0..1);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }

    pub fn add_paint_layer(&mut self, name: String, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.layers.len() >= MAX_LAYERS { return; }
        self.layers.push(Layer::new(name));
        self.active_layer_idx = self.layers.len() - 1;
        self.clear_layer(self.active_layer_idx, device, queue);
        self.compose_layers(device, queue);
    }

    pub fn add_fill_layer(&mut self, name: String, device: &wgpu::Device, queue: &wgpu::Queue, document: &crate::mesh::Document) {
        if self.layers.len() >= MAX_LAYERS { return; }
        let layer = Layer::new_fill(name);
        self.layers.push(layer);
        self.active_layer_idx = self.layers.len() - 1;
        
        let layer_ref = &self.layers[self.active_layer_idx];
        self.render_fill_layer(
            device,
            queue,
            self.active_layer_idx,
            [
                layer_ref.fill_color[0] as f32 / 255.0,
                layer_ref.fill_color[1] as f32 / 255.0,
                layer_ref.fill_color[2] as f32 / 255.0,
                layer_ref.fill_color[3] as f32 / 255.0,
            ],
            [
                layer_ref.fill_noise_color[0] as f32 / 255.0,
                layer_ref.fill_noise_color[1] as f32 / 255.0,
                layer_ref.fill_noise_color[2] as f32 / 255.0,
                layer_ref.fill_noise_color[3] as f32 / 255.0,
            ],
            layer_ref.fill_noise_scale,
            layer_ref.fill_projection_mode,
            document,
        );
        self.compose_layers(device, queue);
    }

    pub fn delete_layer(&mut self, index: usize, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.layers.len() <= 1 { return; }
        
        self.layers.remove(index);
        if self.active_layer_idx >= self.layers.len() {
            self.active_layer_idx = self.layers.len() - 1;
        }
        
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for i in index..self.layers.len() {
            for t in 0..MAX_UDIMS {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.layer_array_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: ((i + 1) * MAX_UDIMS + t) as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.layer_array_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: (i * MAX_UDIMS + t) as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                );
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        
        self.clear_layer(self.layers.len(), device, queue);
        self.compose_layers(device, queue);
    }
    
    pub fn clear_layer(&self, index: usize, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for t in 0..MAX_UDIMS {
            let view_idx = index * MAX_UDIMS + t;
            let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Layer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.layer_views[view_idx],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn load_uv_grid_layer(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.layers.len() >= MAX_LAYERS {
            log::warn!("Maximum layers reached");
            return;
        }

        let img = match image::open("uv_grid.png") {
            Ok(img) => img.into_rgba8(),
            Err(e) => {
                log::error!("Failed to load uv_grid.png: {}", e);
                return;
            }
        };

        if img.width() != self.width || img.height() != self.height {
            log::error!("uv_grid.png dimensions do not match canvas");
            return;
        }

        let layer_idx = self.layers.len();
        self.layers.push(Layer::new("UV Grid".to_string()));
        
        // Write grid to UDIM 1001 (tile index 0)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.layer_array_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: (layer_idx * MAX_UDIMS) as u32 },
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.width),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );

        // Clear tiles 1002, 1003, 1004 to transparent for this layer
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for t in 1..MAX_UDIMS {
            let view_idx = layer_idx * MAX_UDIMS + t;
            let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Grid Layer Tiles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.layer_views[view_idx],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }
        queue.submit(std::iter::once(encoder.finish()));

        self.compose_layers(device, queue);
    }

    pub fn clear_all_layers(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for i in 0..self.layers.len() {
            self.layers[i].strokes.clear();
            for t in 0..MAX_UDIMS {
                let view_idx = i * MAX_UDIMS + t;
                let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Layer Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_views[view_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        self.compose_layers(device, queue);
    }

    pub fn render_fill_layer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer_idx: usize,
        base_color: [f32; 4],
        noise_color: [f32; 4],
        noise_scale: f32,
        projection_mode: u32,
        document: &crate::mesh::Document,
    ) {
        for t in 0..MAX_UDIMS {
            let uniforms = FillUniforms {
                base_color,
                noise_color,
                noise_scale,
                projection_mode,
                udim_tile: t as u32,
                padding: 0,
            };
            queue.write_buffer(&self.fill_uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            let fill_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Fill Bind Group"),
                layout: &self.fill_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.fill_uniform_buffer.as_entire_binding(),
                }],
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(&format!("Fill Render Encoder Layer {} Tile {}", layer_idx, t)),
            });

            {
                let view_idx = layer_idx * MAX_UDIMS + t;
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Fill Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_views[view_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });

                rpass.set_pipeline(&self.fill_pipeline);
                rpass.set_bind_group(0, &fill_bg, &[]);

                let nodes = document.get_active_nodes();
                for (node, _) in &nodes {
                    if let Some(ref mesh) = node.mesh {
                        rpass.set_bind_group(1, &node.bind_group, &[]);
                        for primitive in &mesh.primitives {
                            rpass.set_vertex_buffer(0, primitive.vertex_buffer.slice(..));
                            rpass.set_index_buffer(primitive.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                            rpass.draw_indexed(0..primitive.num_indices, 0, 0..1);
                        }
                    }
                }
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }

    pub fn paint_stroke(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from: glam::Vec2,
        to: glam::Vec2,
        from_3d: Option<glam::Vec3>,
        to_3d: Option<glam::Vec3>,
        color: [u8; 4],
        radius: f32,
        hardness: f32,
        is_eraser: bool,
        num_udim_tiles: u32,
    ) {
        let total_start = std::time::Instant::now();
        
        let color_f32 = [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        ];

        let uv_radius_x = radius / self.width as f32;
        let mut tile_stamps: Vec<Vec<StampInstance>> = vec![Vec::new(); MAX_UDIMS];

        let w = num_udim_tiles as f32;

        let mut add_stamp_udim = |x: f32, y: f32| {
            let wrapped_x = x.rem_euclid(w);

            let x_min = wrapped_x - uv_radius_x;
            let x_max = wrapped_x + uv_radius_x;
            let start_tile = x_min.floor() as i32;
            let end_tile = x_max.floor() as i32;

            for t in start_tile..=end_tile {
                if t >= 0 && t < MAX_UDIMS as i32 {
                    let local_x = wrapped_x - t as f32;
                    tile_stamps[t as usize].push(StampInstance {
                        position: [local_x, y],
                        color: color_f32,
                        radius,
                        hardness,
                    });
                }
            }

            // --- SEAM CROSSING SUPPORT ---
            if wrapped_x - uv_radius_x < 0.0 {
                let wrapped_x_right = wrapped_x + w;
                let x_min_r = wrapped_x_right - uv_radius_x;
                let x_max_r = wrapped_x_right + uv_radius_x;
                let start_tile_r = x_min_r.floor() as i32;
                let end_tile_r = x_max_r.floor() as i32;
                for t in start_tile_r..=end_tile_r {
                    if t >= 0 && t < MAX_UDIMS as i32 {
                        let local_x = wrapped_x_right - t as f32;
                        tile_stamps[t as usize].push(StampInstance {
                            position: [local_x, y],
                            color: color_f32,
                            radius,
                            hardness,
                        });
                    }
                }
            }
            if wrapped_x + uv_radius_x > w {
                let wrapped_x_left = wrapped_x - w;
                let x_min_l = wrapped_x_left - uv_radius_x;
                let x_max_l = wrapped_x_left + uv_radius_x;
                let start_tile_l = x_min_l.floor() as i32;
                let end_tile_l = x_max_l.floor() as i32;
                for t in start_tile_l..=end_tile_l {
                    if t >= 0 && t < MAX_UDIMS as i32 {
                        let local_x = wrapped_x_left - t as f32;
                        tile_stamps[t as usize].push(StampInstance {
                            position: [local_x, y],
                            color: color_f32,
                            radius,
                            hardness,
                        });
                    }
                }
            }
        };

        let accum_start = std::time::Instant::now();
        
        let is_wrap = if let (Some(f3d), Some(t3d)) = (from_3d, to_3d) {
            let dist_3d = f3d.distance(t3d);
            let dist_uv = (to.x - from.x).abs();
            dist_3d < 0.5 && dist_uv > 0.5 * w
        } else {
            false
        };

        let mut dx = to.x - from.x;
        let dy = to.y - from.y;
        if is_wrap {
            dx = dx - w * (dx / w).round();
        }

        let from_px = from * glam::Vec2::new(self.width as f32, self.height as f32);
        let to_px_effective = (from + glam::Vec2::new(dx, dy)) * glam::Vec2::new(self.width as f32, self.height as f32);
        let dist = from_px.distance(to_px_effective);

        let step_size = (radius * 0.1).max(1.0);
        let num_steps = (dist / step_size).ceil() as u32;

        if num_steps <= 1 {
            add_stamp_udim(to.x, to.y);
        } else {
            for step in 0..=num_steps {
                let t = step as f32 / num_steps as f32;
                let x = from.x + dx * t;
                let y = from.y + dy * t;
                add_stamp_udim(x, y);
            }
        }
        let accum_duration = accum_start.elapsed();

        let render_start = std::time::Instant::now();
        for t in 0..MAX_UDIMS {
            let stamps = &tile_stamps[t];
            if stamps.is_empty() { continue; }

            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Brush Instance Buffer Tile {}", t)),
                contents: bytemuck::cast_slice(stamps),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Brush Render Encoder"),
            });

            {
                let view_idx = self.active_layer_idx * MAX_UDIMS + t;
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Brush Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_views[view_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                rpass.set_pipeline(if is_eraser { &self.brush_pipeline_sub } else { &self.brush_pipeline_add });
                rpass.set_bind_group(0, &self.brush_bind_group, &[]);
                rpass.set_vertex_buffer(0, instance_buffer.slice(..));
                rpass.draw(0..4, 0..stamps.len() as u32);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
        let render_duration = render_start.elapsed();
        
        let compose_start = std::time::Instant::now();
        self.compose_layers(device, queue);
        let compose_duration = compose_start.elapsed();

        log::debug!(
            "paint_stroke detailed timing: accum={:?}, render={:?}, compose={:?}, total={:?}, stamps={}",
            accum_duration,
            render_duration,
            compose_duration,
            total_start.elapsed(),
            num_steps + 1
        );
    }

    pub fn paint_stamp(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uv: glam::Vec2,
        pos: Option<glam::Vec3>,
        color: [u8; 4],
        radius: f32,
        hardness: f32,
        is_eraser: bool,
        num_udim_tiles: u32,
    ) {
        self.paint_stroke(
            device,
            queue,
            uv,
            uv,
            pos,
            pos,
            color,
            radius,
            hardness,
            is_eraser,
            num_udim_tiles,
        );
    }

    pub fn paint_stroke_udim_to_layer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer_idx: usize,
        stroke: &PaintStroke,
        num_udim_tiles: u32,
    ) {
        if stroke.uv_points.is_empty() { return; }
        
        let color_f32 = [
            stroke.color[0] as f32 / 255.0,
            stroke.color[1] as f32 / 255.0,
            stroke.color[2] as f32 / 255.0,
            stroke.color[3] as f32 / 255.0,
        ];
        
        let uv_radius_x = stroke.radius / self.width as f32;
        let mut tile_stamps: Vec<Vec<StampInstance>> = vec![Vec::new(); MAX_UDIMS];

        let w = num_udim_tiles as f32;

        let mut add_stamp_udim = |x: f32, y: f32| {
            let wrapped_x = x.rem_euclid(w);

            let x_min = wrapped_x - uv_radius_x;
            let x_max = wrapped_x + uv_radius_x;
            let start_tile = x_min.floor() as i32;
            let end_tile = x_max.floor() as i32;

            for t in start_tile..=end_tile {
                if t >= 0 && t < MAX_UDIMS as i32 {
                    let local_x = wrapped_x - t as f32;
                    tile_stamps[t as usize].push(StampInstance {
                        position: [local_x, y],
                        color: color_f32,
                        radius: stroke.radius,
                        hardness: stroke.hardness,
                    });
                }
            }

            // --- SEAM CROSSING SUPPORT ---
            if wrapped_x - uv_radius_x < 0.0 {
                let wrapped_x_right = wrapped_x + w;
                let x_min_r = wrapped_x_right - uv_radius_x;
                let x_max_r = wrapped_x_right + uv_radius_x;
                let start_tile_r = x_min_r.floor() as i32;
                let end_tile_r = x_max_r.floor() as i32;
                for t in start_tile_r..=end_tile_r {
                    if t >= 0 && t < MAX_UDIMS as i32 {
                        let local_x = wrapped_x_right - t as f32;
                        tile_stamps[t as usize].push(StampInstance {
                            position: [local_x, y],
                            color: color_f32,
                            radius: stroke.radius,
                            hardness: stroke.hardness,
                        });
                    }
                }
            }
            if wrapped_x + uv_radius_x > w {
                let wrapped_x_left = wrapped_x - w;
                let x_min_l = wrapped_x_left - uv_radius_x;
                let x_max_l = wrapped_x_left + uv_radius_x;
                let start_tile_l = x_min_l.floor() as i32;
                let end_tile_l = x_max_l.floor() as i32;
                for t in start_tile_l..=end_tile_l {
                    if t >= 0 && t < MAX_UDIMS as i32 {
                        let local_x = wrapped_x_left - t as f32;
                        tile_stamps[t as usize].push(StampInstance {
                            position: [local_x, y],
                            color: color_f32,
                            radius: stroke.radius,
                            hardness: stroke.hardness,
                        });
                    }
                }
            }
        };

        let points_count = stroke.uv_points.len();
        if points_count == 1 {
            add_stamp_udim(stroke.uv_points[0].x, stroke.uv_points[0].y);
        } else {
            for i in 0..points_count - 1 {
                let from = stroke.uv_points[i];
                let to = stroke.uv_points[i + 1];
                let f3d = stroke.points.get(i).copied();
                let t3d = stroke.points.get(i + 1).copied();

                let is_wrap = if let (Some(f3d_pt), Some(t3d_pt)) = (f3d, t3d) {
                    let dist_3d = f3d_pt.distance(t3d_pt);
                    let dist_uv = (to.x - from.x).abs();
                    dist_3d < 0.5 && dist_uv > 0.5 * w
                } else {
                    false
                };

                let mut dx = to.x - from.x;
                let dy = to.y - from.y;
                if is_wrap {
                    dx = dx - w * (dx / w).round();
                }

                let from_px = from * glam::Vec2::new(self.width as f32, self.height as f32);
                let to_px_effective = (from + glam::Vec2::new(dx, dy)) * glam::Vec2::new(self.width as f32, self.height as f32);
                let dist = from_px.distance(to_px_effective);

                let step_size = (stroke.radius * 0.1).max(1.0);
                let num_steps = (dist / step_size).ceil() as u32;

                if num_steps <= 1 {
                    add_stamp_udim(to.x, to.y);
                } else {
                    for step in 0..=num_steps {
                        let t = step as f32 / num_steps as f32;
                        let x = from.x + dx * t;
                        let y = from.y + dy * t;
                        add_stamp_udim(x, y);
                    }
                }
            }
        }

        for t in 0..MAX_UDIMS {
            let stamps = &tile_stamps[t];
            if stamps.is_empty() { continue; }

            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Brush Instance Buffer Layer {} Tile {}", layer_idx, t)),
                contents: bytemuck::cast_slice(stamps),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Brush Render Encoder"),
            });

            {
                let view_idx = layer_idx * MAX_UDIMS + t;
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Brush Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_views[view_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                rpass.set_pipeline(if stroke.is_eraser { &self.brush_pipeline_sub } else { &self.brush_pipeline_add });
                rpass.set_bind_group(0, &self.brush_bind_group, &[]);
                rpass.set_vertex_buffer(0, instance_buffer.slice(..));
                rpass.draw(0..4, 0..stamps.len() as u32);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }

    pub fn reproject_strokes(&mut self, document: &crate::mesh::Document) {
        let nodes = document.get_active_nodes();
        
        for layer in &mut self.layers {
            if layer.is_fill { continue; }
            for stroke in &mut layer.strokes {
                stroke.uv_points.clear();
                
                for pt in &stroke.points {
                    let mut closest_uv = glam::Vec2::ZERO;
                    let mut min_dist_sq = f32::MAX;
                    
                    for (node, world_matrix) in &nodes {
                        if let Some(ref mesh) = node.mesh {
                            for primitive in &mesh.primitives {
                                for chunk in primitive.indices.chunks_exact(3) {
                                    let i0 = chunk[0] as usize;
                                    let i1 = chunk[1] as usize;
                                    let i2 = chunk[2] as usize;
                                    
                                    let v0 = &primitive.vertices[i0];
                                    let v1 = &primitive.vertices[i1];
                                    let v2 = &primitive.vertices[i2];
                                    
                                    let p0 = world_matrix.transform_point3(glam::Vec3::from(v0.position));
                                    let p1 = world_matrix.transform_point3(glam::Vec3::from(v1.position));
                                    let p2 = world_matrix.transform_point3(glam::Vec3::from(v2.position));
                                    
                                    let (closest_pt, bary) = closest_point_on_triangle(*pt, p0, p1, p2);
                                    let dist_sq = pt.distance_squared(closest_pt);
                                    
                                    if dist_sq < min_dist_sq {
                                        min_dist_sq = dist_sq;
                                        let uv0 = glam::Vec2::from(v0.tex_coords);
                                        let uv1 = glam::Vec2::from(v1.tex_coords);
                                        let uv2 = glam::Vec2::from(v2.tex_coords);
                                        closest_uv = uv0 * bary[0] + uv1 * bary[1] + uv2 * bary[2];
                                    }
                                }
                            }
                        }
                    }
                    stroke.uv_points.push(closest_uv);
                }
            }
        }
    }

    pub fn redraw_all_layers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        document: &crate::mesh::Document,
    ) {
        // Clear all layers on the GPU first
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for i in 0..self.layers.len() {
            for t in 0..MAX_UDIMS {
                let view_idx = i * MAX_UDIMS + t;
                let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Layer Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_views[view_idx],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
            }
        }
        queue.submit(std::iter::once(encoder.finish()));

        for idx in 0..self.layers.len() {
            let is_fill = self.layers[idx].is_fill;
            if is_fill {
                let layer = &self.layers[idx];
                self.render_fill_layer(
                    device,
                    queue,
                    idx,
                    [
                        layer.fill_color[0] as f32 / 255.0,
                        layer.fill_color[1] as f32 / 255.0,
                        layer.fill_color[2] as f32 / 255.0,
                        layer.fill_color[3] as f32 / 255.0,
                    ],
                    [
                        layer.fill_noise_color[0] as f32 / 255.0,
                        layer.fill_noise_color[1] as f32 / 255.0,
                        layer.fill_noise_color[2] as f32 / 255.0,
                        layer.fill_noise_color[3] as f32 / 255.0,
                    ],
                    layer.fill_noise_scale,
                    layer.fill_projection_mode,
                    document,
                );
            } else {
                let strokes = self.layers[idx].strokes.clone();
                for stroke in &strokes {
                    self.paint_stroke_udim_to_layer(device, queue, idx, stroke, document.num_udim_tiles);
                }
            }
        }
        
        self.compose_layers(device, queue);
    }

    pub fn export_png(&self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) {
        // Export each of the 4 UDIM tiles as separate files!
        // Path matches e.g. "painted_texture.png" -> exports as "painted_texture_1001.png", "painted_texture_1002.png", etc.
        let path_stem = if path.ends_with(".png") {
            &path[..path.len() - 4]
        } else {
            path
        };

        for tile_idx in 0..MAX_UDIMS {
            let udim_num = 1001 + tile_idx;
            let export_path = format!("{}_{}.png", path_stem, udim_num);

            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let unpadded_bytes_per_row = self.width * 4;
            let padding = (align - unpadded_bytes_per_row % align) % align;
            let padded_bytes_per_row = unpadded_bytes_per_row + padding;
            
            let buffer_size = (padded_bytes_per_row * self.height) as wgpu::BufferAddress;
            
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Export Buffer Tile {}", udim_num)),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: tile_idx as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row),
                        rows_per_image: Some(self.height),
                    },
                },
                wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
            queue.submit(std::iter::once(encoder.finish()));
            
            let buffer_slice = buffer.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            
            if let Ok(Ok(())) = rx.recv() {
                let padded_data = buffer_slice.get_mapped_range();
                let mut unpadded_data = Vec::with_capacity((self.width * self.height * 4) as usize);
                for chunk in padded_data.chunks(padded_bytes_per_row as usize) {
                    unpadded_data.extend_from_slice(&chunk[..unpadded_bytes_per_row as usize]);
                }
                drop(padded_data);
                buffer.unmap();
                
                if let Err(e) = image::save_buffer(&export_path, &unpadded_data, self.width, self.height, image::ColorType::Rgba8) {
                    log::error!("Failed to export UDIM texture {}: {:?}", udim_num, e);
                } else {
                    log::info!("Successfully exported UDIM texture to {}", export_path);
                }
            }
        }
    }
}

// Closest point on a triangle ABC to point P (computes barycentric coordinates)
fn closest_point_on_triangle(p: glam::Vec3, a: glam::Vec3, b: glam::Vec3, c: glam::Vec3) -> (glam::Vec3, [f32; 3]) {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + v * ab, [1.0 - v, v, 0.0]);
    }
    
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + w * ac, [1.0 - w, 0.0, w]);
    }
    
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + w * (c - b), [0.0, 1.0 - w, w]);
    }
    
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2Array, // Must be D2Array for multi-UDIM
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
        label: Some("Painter Texture Bind Group Layout"),
    })
}
