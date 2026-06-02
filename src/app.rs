use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::Window;

pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,

    // Render pipeline & logic state
    viewport: crate::viewport::Viewport,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pub painter: crate::painter::Painter,

    // Brush parameters
    pub brush_size: f32,
    pub brush_color: [u8; 4],
    pub brush_hardness: f32,

    // Camera movement/interaction mouse state
    is_left_clicked: bool,
    is_right_clicked: bool,
    last_mouse_pos: winit::dpi::PhysicalPosition<f64>,

    // Keyboard navigation modifiers
    is_space_pressed: bool,
    is_alt_pressed: bool,

    // Stroke tracking
    last_hit_uv: Option<glam::Vec2>,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        log::info!("Creating State with window size: {}x{}", size.width, size.height);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create texture bind group layout (shared with painter and viewport)
        let texture_bind_group_layout = crate::painter::create_bind_group_layout(&device);

        // Create painter
        let painter = crate::painter::Painter::new(&device, &texture_bind_group_layout);
        painter.write_to_gpu(&queue);

        // Create 3D Viewport
        let aspect = size.width as f32 / size.height as f32;
        let viewport = crate::viewport::Viewport::new(
            &device,
            surface_format,
            aspect,
            &texture_bind_group_layout,
        );

        // Create depth texture
        let (depth_texture, depth_view) = crate::viewport::create_depth_texture(&device, &config, "depth_texture");

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            viewport,
            depth_texture,
            depth_view,
            painter,
            brush_size: 25.0,
            brush_color: [220, 50, 50, 255], // bright red default
            brush_hardness: 0.5,
            is_left_clicked: false,
            is_right_clicked: false,
            last_mouse_pos: winit::dpi::PhysicalPosition::new(0.0, 0.0),
            is_space_pressed: false,
            is_alt_pressed: false,
            last_hit_uv: None,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            // Recreate depth texture
            let (depth_texture, depth_view) = crate::viewport::create_depth_texture(&self.device, &self.config, "depth_texture");
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            // Update aspect ratio in camera
            self.viewport.camera.aspect = new_size.width as f32 / new_size.height as f32;

            log::info!("Resized to: {}x{}", new_size.width, new_size.height);
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event: winit::event::KeyEvent {
                    physical_key,
                    state,
                    ..
                },
                ..
            } => {
                let pressed = *state == winit::event::ElementState::Pressed;
                match physical_key {
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Space) => {
                        self.is_space_pressed = pressed;
                        true
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::AltLeft) |
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::AltRight) => {
                        self.is_alt_pressed = pressed;
                        true
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::BracketLeft) => {
                        if pressed {
                            self.brush_size = (self.brush_size - 5.0).max(2.0);
                            log::info!("Brush size: {}", self.brush_size);
                        }
                        true
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::BracketRight) => {
                        if pressed {
                            self.brush_size = (self.brush_size + 5.0).min(300.0);
                            log::info!("Brush size: {}", self.brush_size);
                        }
                        true
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) => {
                        if pressed {
                            self.painter.fill([230, 230, 230, 255]);
                            self.painter.write_to_gpu(&self.queue);
                            log::info!("Cleared canvas");
                        }
                        true
                    }
                    _ => false,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == winit::event::ElementState::Pressed;
                match button {
                    winit::event::MouseButton::Left => {
                        self.is_left_clicked = pressed;
                        if !pressed {
                            self.last_hit_uv = None; // Reset brush stroke
                        } else {
                            // If we aren't navigation-dragging, paint immediately on click
                            if !self.is_space_pressed && !self.is_alt_pressed {
                                self.paint_at_cursor();
                            }
                        }
                        true
                    }
                    winit::event::MouseButton::Right => {
                        self.is_right_clicked = pressed;
                        true
                    }
                    _ => false,
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let dx = position.x - self.last_mouse_pos.x;
                let dy = position.y - self.last_mouse_pos.y;
                self.last_mouse_pos = *position;

                let is_navigating = self.is_space_pressed || self.is_alt_pressed;

                if self.is_left_clicked {
                    if is_navigating {
                        // Orbit camera (left-drag + space/alt)
                        self.viewport.camera.yaw -= (dx * 0.005) as f32;
                        self.viewport.camera.pitch = (self.viewport.camera.pitch + (dy * 0.005) as f32)
                            .clamp(-std::f32::consts::FRAC_PI_2 + 0.05, std::f32::consts::FRAC_PI_2 - 0.05);
                        self.last_hit_uv = None; // Break stroke
                    } else {
                        // Paint on model (normal left-drag)
                        self.paint_at_cursor();
                    }
                    return true;
                }

                if self.is_right_clicked {
                    // Pan camera (right-drag)
                    let eye = self.viewport.camera.get_eye();
                    let forward = (self.viewport.camera.target - eye).normalize();
                    let right = forward.cross(glam::Vec3::Y).normalize();
                    let up = right.cross(forward).normalize();

                    let speed = self.viewport.camera.distance * 0.0015;
                    self.viewport.camera.target += right * (-dx as f32 * speed) + up * (dy as f32 * speed);
                    return true;
                }

                false
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.05,
                };
                self.viewport.camera.distance = (self.viewport.camera.distance - scroll * 0.25).max(1.0).min(50.0);
                true
            }
            _ => false,
        }
    }

    fn paint_at_cursor(&mut self) {
        let mouse_pos = glam::Vec2::new(self.last_mouse_pos.x as f32, self.last_mouse_pos.y as f32);
        let screen_size = glam::Vec2::new(self.size.width as f32, self.size.height as f32);

        let eye = self.viewport.camera.get_eye();
        let view = glam::Mat4::look_at_rh(eye, self.viewport.camera.target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(
            self.viewport.camera.fovy,
            self.viewport.camera.aspect,
            self.viewport.camera.znear,
            self.viewport.camera.zfar,
        );

        let ray = crate::raycast::Ray::from_screen(mouse_pos, screen_size, view, proj);

        if let Some(hit) = crate::raycast::intersect_mesh(
            &ray,
            &self.viewport.mesh_vertices,
            &self.viewport.mesh_indices,
        ) {
            if let Some(last_uv) = self.last_hit_uv {
                self.painter.paint_stroke(
                    last_uv,
                    hit.uv,
                    self.brush_color,
                    self.brush_size,
                    self.brush_hardness,
                );
            } else {
                self.painter.paint_stamp(
                    hit.uv,
                    self.brush_color,
                    self.brush_size,
                    self.brush_hardness,
                );
            }
            self.last_hit_uv = Some(hit.uv);
            self.painter.write_to_gpu(&self.queue);
        } else {
            self.last_hit_uv = None;
        }
    }

    pub fn update(&mut self) {
        // Subtle light rotation over time to show dynamic shading
        self.viewport.light_angle += 0.005;
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Update uniforms on GPU
        self.viewport.update_camera(&self.queue);

        // Render viewport (Pass 1)
        self.viewport.render(
            &mut encoder,
            &view,
            &self.depth_view,
            &self.painter.bind_group,
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
