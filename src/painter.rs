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

pub struct Layer {
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub blend_mode: BlendMode,
}

impl Layer {
    pub fn new(name: String) -> Self {
        Self {
            name,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
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
    pub padding: [u32; 3],
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

pub const MAX_LAYERS: usize = 16;

pub struct Painter {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer_idx: usize,
    
    // WGPU Resources
    pub layer_array_texture: wgpu::Texture,
    pub layer_views: Vec<wgpu::TextureView>,
    
    pub texture: wgpu::Texture, // composite texture
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    
    brush_pipeline_add: wgpu::RenderPipeline,
    brush_pipeline_sub: wgpu::RenderPipeline,
    brush_uniform_buffer: wgpu::Buffer,
    brush_bind_group: wgpu::BindGroup,
    
    composite_pipeline: wgpu::RenderPipeline,
    composite_uniform_buffer: wgpu::Buffer,
    composite_bind_group: wgpu::BindGroup,
}

impl Painter {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let width = 1024;
        let height = 1024;
        
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let array_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: MAX_LAYERS as u32,
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
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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
        for i in 0..MAX_LAYERS {
            layer_views.push(layer_array_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("Layer {} View", i)),
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
            bind_group_layouts: &[&brush_bind_group_layout],
            push_constant_ranges: &[],
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

        // Brush uses premultiplied alpha blending
        let brush_pipeline_add = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Brush Pipeline Add"),
            layout: Some(&brush_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &brush_shader,
                entry_point: "vs_main",
                buffers: &[instance_desc.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &brush_shader,
                entry_point: "fs_main",
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
            multiview: None,
        });

        // Eraser uses subtractive alpha blending
        let brush_pipeline_sub = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Brush Pipeline Sub"),
            layout: Some(&brush_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &brush_shader,
                entry_point: "vs_main",
                buffers: &[instance_desc.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &brush_shader,
                entry_point: "fs_main",
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
            multiview: None,
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
            bind_group_layouts: &[&composite_bind_group_layout],
            push_constant_ranges: &[],
        });

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None, // We replace the entire composite texture
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
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
        }
    }

    pub fn compose_layers(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut uniform_data = LayersUniform {
            count: self.layers.len() as u32,
            padding: [0; 3],
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
            label: Some("Composite Encoder"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture_view,
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
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rpass.set_pipeline(&self.composite_pipeline);
            rpass.set_bind_group(0, &self.composite_bind_group, &[]);
            rpass.draw(0..3, 0..1); // 3 vertices for fullscreen triangle
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn add_layer(&mut self, name: String, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.layers.len() >= MAX_LAYERS { return; }
        self.layers.push(Layer::new(name));
        self.active_layer_idx = self.layers.len() - 1;
        self.clear_layer(self.active_layer_idx, device, queue);
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
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.layer_array_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: (i + 1) as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &self.layer_array_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
        }
        queue.submit(std::iter::once(encoder.finish()));
        
        self.clear_layer(self.layers.len(), device, queue);
        self.compose_layers(device, queue);
    }
    
    pub fn clear_layer(&self, index: usize, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Layer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.layer_views[index],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
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
            log::error!("uv_grid.png dimensions ({}x{}) do not match canvas ({}x{})", img.width(), img.height(), self.width, self.height);
            return;
        }

        let layer_idx = self.layers.len() as u32;

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.layer_array_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer_idx,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.width),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.layers.push(Layer {
            name: "UV Grid".to_string(),
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
        });

        self.compose_layers(device, queue);
    }

    pub fn clear_all_layers(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for i in 0..self.layers.len() {
            let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Layer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.layer_views[i],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        queue.submit(std::iter::once(encoder.finish()));
        self.compose_layers(device, queue);
    }

    pub fn paint_stroke(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, from: glam::Vec2, to: glam::Vec2, color: [u8; 4], radius: f32, hardness: f32, is_eraser: bool) {
        // Adjust for horizontal wrapping (X-coordinate)
        let mut from_x = from.x;
        let mut to_x = to.x;
        if (from_x - to_x).abs() > 0.5 {
            if from_x > to_x {
                to_x += 1.0;
            } else {
                from_x += 1.0;
            }
        }

        // Check if there is still a large vertical jump (V-coordinate seam)
        // or a large horizontal jump that couldn't be resolved
        let dx = from_x - to_x;
        let dy = from.y - to.y;
        let uv_dist_sq = dx * dx + dy * dy;

        // If the remaining distance in UV space is too large (e.g. > 0.25),
        // we treat it as a discontinuous seam crossing and do not interpolate.
        // We only paint the stamp at the current position `to`.
        let is_discontinuous = uv_dist_sq > 0.25 * 0.25;

        let from_px = glam::Vec2::new(from_x, from.y) * glam::Vec2::new(self.width as f32, self.height as f32);
        let to_px = glam::Vec2::new(to_x, to.y) * glam::Vec2::new(self.width as f32, self.height as f32);
        
        let mut instances = Vec::new();
        
        let color_f32 = [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        ];

        let uv_radius_x = radius / self.width as f32;
        let mut add_stamp = |instances: &mut Vec<StampInstance>, x: f32, y: f32| {
            instances.push(StampInstance {
                position: [x, y],
                color: color_f32,
                radius,
                hardness,
            });
            if x - uv_radius_x < 0.0 {
                instances.push(StampInstance {
                    position: [x + 1.0, y],
                    color: color_f32,
                    radius,
                    hardness,
                });
            } else if x + uv_radius_x > 1.0 {
                instances.push(StampInstance {
                    position: [x - 1.0, y],
                    color: color_f32,
                    radius,
                    hardness,
                });
            }
        };

        if is_discontinuous {
            add_stamp(&mut instances, to.x, to.y);
        } else {
            let dist = from_px.distance(to_px);

            // Spacing: stamp every 10% of radius (min 1 pixel)
            let step_size = (radius * 0.1).max(1.0);
            let num_steps = (dist / step_size).ceil() as u32;

            if num_steps <= 1 {
                add_stamp(&mut instances, to.x, to.y);
            } else {
                for step in 0..=num_steps {
                    let t = step as f32 / num_steps as f32;
                    let x = (from_x + (to_x - from_x) * t).rem_euclid(1.0);
                    let y = from.y + (to.y - from.y) * t;
                    add_stamp(&mut instances, x, y);
                }
            }
        }
        
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Brush Instance Buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Brush Render Encoder"),
        });
        
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Brush Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.layer_views[self.active_layer_idx],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rpass.set_pipeline(if is_eraser { &self.brush_pipeline_sub } else { &self.brush_pipeline_add });
            rpass.set_bind_group(0, &self.brush_bind_group, &[]);
            rpass.set_vertex_buffer(0, instance_buffer.slice(..));
            rpass.draw(0..4, 0..instances.len() as u32);
        }
        
        queue.submit(std::iter::once(encoder.finish()));
        
        self.compose_layers(device, queue);
    }

    pub fn paint_stamp(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, uv: glam::Vec2, color: [u8; 4], radius: f32, hardness: f32, is_eraser: bool) {
        self.paint_stroke(device, queue, uv, uv, color, radius, hardness, is_eraser);
    }
    
    pub fn export_png(&self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) {
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded_bytes_per_row = self.width * 4;
        let padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padding;
        
        let buffer_size = (padded_bytes_per_row * self.height) as wgpu::BufferAddress;
        
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
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
            tx.send(result).unwrap();
        });
        
        device.poll(wgpu::Maintain::Wait);
        
        if let Ok(Ok(())) = rx.recv() {
            let padded_data = buffer_slice.get_mapped_range();
            let mut unpadded_data = Vec::with_capacity((self.width * self.height * 4) as usize);
            for chunk in padded_data.chunks(padded_bytes_per_row as usize) {
                unpadded_data.extend_from_slice(&chunk[..unpadded_bytes_per_row as usize]);
            }
            drop(padded_data);
            buffer.unmap();
            
            if let Err(e) = image::save_buffer(path, &unpadded_data, self.width, self.height, image::ColorType::Rgba8) {
                log::error!("Failed to export texture: {:?}", e);
            } else {
                log::info!("Successfully exported composed texture to {}", path);
            }
        }
    }
}

pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
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
