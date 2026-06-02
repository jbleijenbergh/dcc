pub struct Painter {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
}

impl Painter {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let width = 1024;
        let height = 1024;

        // Initialize with a clean base color (e.g. white/light gray)
        let mut pixels = vec![0; (width * height * 4) as usize];
        for i in (0..pixels.len()).step_by(4) {
            pixels[i] = 230;     // R
            pixels[i + 1] = 230; // G
            pixels[i + 2] = 230; // B
            pixels[i + 3] = 255; // A
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Painted Canvas Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("Painter Texture Bind Group"),
        });

        Self {
            width,
            height,
            pixels,
            texture,
            texture_view,
            sampler,
            bind_group,
        }
    }

    pub fn write_to_gpu(&self, queue: &wgpu::Queue) {
        let size = wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        };
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            size,
        );
    }

    pub fn paint_stamp(&mut self, uv: glam::Vec2, color: [u8; 4], radius: f32, hardness: f32) {
        let cx = uv.x * self.width as f32;
        let cy = uv.y * self.height as f32;

        let r = radius;
        let r_sq = r * r;

        let min_x = (cx - r).floor().max(0.0) as u32;
        let max_x = (cx + r).ceil().min(self.width as f32 - 1.0) as u32;
        let min_y = (cy - r).floor().max(0.0) as u32;
        let max_y = (cy + r).ceil().min(self.height as f32 - 1.0) as u32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq <= r_sq {
                    let dist = dist_sq.sqrt();
                    let hardness_radius = r * hardness;
                    
                    let intensity = if dist <= hardness_radius {
                        1.0
                    } else if r > hardness_radius {
                        1.0 - (dist - hardness_radius) / (r - hardness_radius)
                    } else {
                        0.0;
                    };

                    let idx = ((y * self.width + x) * 4) as usize;
                    let brush_alpha = (color[3] as f32 * intensity) / 255.0;

                    // Standard alpha blend compositing onto canvas
                    self.pixels[idx] = ((color[0] as f32 * brush_alpha) + (self.pixels[idx] as f32 * (1.0 - brush_alpha))) as u8;
                    self.pixels[idx + 1] = ((color[1] as f32 * brush_alpha) + (self.pixels[idx + 1] as f32 * (1.0 - brush_alpha))) as u8;
                    self.pixels[idx + 2] = ((color[2] as f32 * brush_alpha) + (self.pixels[idx + 2] as f32 * (1.0 - brush_alpha))) as u8;
                    self.pixels[idx + 3] = ((255.0 * brush_alpha) + (self.pixels[idx + 3] as f32 * (1.0 - brush_alpha))) as u8;
                }
            }
        }
    }

    pub fn paint_stroke(&mut self, from: glam::Vec2, to: glam::Vec2, color: [u8; 4], radius: f32, hardness: f32) {
        let from_px = from * glam::Vec2::new(self.width as f32, self.height as f32);
        let to_px = to * glam::Vec2::new(self.width as f32, self.height as f32);
        let dist = from_px.distance(to_px);

        // Spacing: stamp every 10% of radius (min 1 pixel)
        let step_size = (radius * 0.1).max(1.0);
        let num_steps = (dist / step_size).ceil() as u32;

        if num_steps <= 1 {
            self.paint_stamp(to, color, radius, hardness);
            return;
        }

        for step in 0..=num_steps {
            let t = step as f32 / num_steps as f32;
            let uv = from.lerp(to, t);
            self.paint_stamp(uv, color, radius, hardness);
        }
    }

    pub fn fill(&mut self, color: [u8; 4]) {
        for i in (0..self.pixels.len()).step_by(4) {
            self.pixels[i] = color[0];
            self.pixels[i + 1] = color[1];
            self.pixels[i + 2] = color[2];
            self.pixels[i + 3] = color[3];
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
