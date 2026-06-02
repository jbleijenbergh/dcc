use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::Window;
use crate::painter::BlendMode;

#[derive(Debug)]
pub enum SurfaceError {
    Lost,
    Outdated,
    Timeout,
    Other(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Eraser,
}

pub struct State {
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,

    // Render pipeline & logic state
    viewport: crate::viewport::Viewport,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pub painter: crate::painter::Painter,

    // Brush parameters
    pub brush_size: f32,
    pub brush_color: [u8; 4],
    pub brush_hardness: f32,
    pub brush_opacity: f32,
    pub active_tool: Tool,

    // Layer stack UI state
    new_layer_name: String,

    // Geometry selection
    pub current_mesh_type: String,

    // Egui state
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

    // Camera movement/interaction mouse state
    is_left_clicked: bool,
    is_right_clicked: bool,
    last_mouse_pos: winit::dpi::PhysicalPosition<f64>,

    // Keyboard navigation modifiers
    is_space_pressed: bool,
    is_alt_pressed: bool,

    // Stroke tracking
    last_hit_uv: Option<glam::Vec2>,

    // Async loading state
    gltf_rx: Option<std::sync::mpsc::Receiver<Result<(crate::mesh::Document, String), String>>>,
    is_loading_gltf: bool,

    // Paint scheduling
    needs_paint: bool,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        log::info!("Creating State with window size: {}x{}", size.width, size.height);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
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
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: None,
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .unwrap();
        let device = Arc::new(device);

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
        let mut painter = crate::painter::Painter::new(&device, &texture_bind_group_layout);
        painter.clear_all_layers(&device, &queue);

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

        // Initialize Egui
        let egui_ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        egui_ctx.set_fonts(fonts);

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            egui_wgpu::RendererOptions {
                depth_stencil_format: None,
                msaa_samples: 1,
                ..Default::default()
            },
        );

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
            brush_opacity: 1.0,
            active_tool: Tool::Brush,
            new_layer_name: String::new(),
            current_mesh_type: "Sphere".to_string(),
            egui_ctx,
            egui_state,
            egui_renderer,
            is_left_clicked: false,
            is_right_clicked: false,
            last_mouse_pos: winit::dpi::PhysicalPosition::new(0.0, 0.0),
            is_space_pressed: false,
            is_alt_pressed: false,
            last_hit_uv: None,
            gltf_rx: None,
            is_loading_gltf: false,
            needs_paint: false,
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
        // Always reset paint stroke tracking on left mouse button release, even if egui consumes it
        if let WindowEvent::MouseInput { state: winit::event::ElementState::Released, button: winit::event::MouseButton::Left, .. } = event {
            self.last_hit_uv = None;
            self.is_left_clicked = false;
        }

        // Let egui intercept events first
        let egui_resp = self.egui_state.on_window_event(&*self.window, event);
        if egui_resp.consumed {
            return true;
        }

        match event {
            WindowEvent::KeyboardInput {
                event: winit::event::KeyEvent {
                    physical_key,
                    state,
                    ..
                },
                ..
            } => {
                if self.egui_ctx.egui_wants_keyboard_input() {
                    return false;
                }

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
                            self.painter.clear_all_layers(&self.device, &self.queue);
                            log::info!("Cleared canvas");
                        }
                        true
                    }
                    _ => false,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.egui_ctx.egui_wants_pointer_input() {
                    return false;
                }

                let pressed = *state == winit::event::ElementState::Pressed;
                match button {
                    winit::event::MouseButton::Left => {
                        self.is_left_clicked = pressed;
                        if !pressed {
                            self.last_hit_uv = None; // Reset brush stroke
                        } else {
                            // If we aren't navigation-dragging, paint immediately on click
                            if !self.is_space_pressed && !self.is_alt_pressed {
                                self.needs_paint = true;
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

                if self.egui_ctx.egui_wants_pointer_input() {
                    return false;
                }

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
                        self.needs_paint = true;
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
                if self.egui_ctx.egui_wants_pointer_input() {
                    return false;
                }

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
        if self.egui_ctx.egui_wants_pointer_input() {
            return;
        }

        let start_time = std::time::Instant::now();

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

        let is_eraser = self.active_tool == Tool::Eraser;
        let mut brush_rgba = self.brush_color;
        brush_rgba[3] = (self.brush_opacity * 255.0) as u8;

        let raycast_start = std::time::Instant::now();
        let hit_opt = crate::raycast::intersect_document(
            &ray,
            &self.viewport.document,
        );
        let raycast_duration = raycast_start.elapsed();

        if let Some(hit) = hit_opt {
            let paint_start = std::time::Instant::now();
            if let Some(last_uv) = self.last_hit_uv {
                self.painter.paint_stroke(
                    &self.device,
                    &self.queue,
                    last_uv,
                    hit.uv,
                    brush_rgba,
                    self.brush_size,
                    self.brush_hardness,
                    is_eraser,
                );
            } else {
                log::info!(
                    "Mouse click / Stroke start coordinates:\n  Screen: [x: {:.2}, y: {:.2}]\n  World:  [x: {:.4}, y: {:.4}, z: {:.4}]\n  UV:     [u: {:.4}, v: {:.4}]",
                    mouse_pos.x,
                    mouse_pos.y,
                    hit.point.x,
                    hit.point.y,
                    hit.point.z,
                    hit.uv.x,
                    hit.uv.y
                );
                self.painter.paint_stamp(
                    &self.device,
                    &self.queue,
                    hit.uv,
                    brush_rgba,
                    self.brush_size,
                    self.brush_hardness,
                    is_eraser,
                );
            }
            let paint_duration = paint_start.elapsed();
            self.last_hit_uv = Some(hit.uv);

            log::info!(
                "Paint stroke timing: raycast={:?}, paint={:?}, total={:?}",
                raycast_duration,
                paint_duration,
                start_time.elapsed()
            );
        } else {
            self.last_hit_uv = None;
        }
    }

    fn toggle_mesh(&mut self, mode: &str) {
        let doc = match mode {
            "Cube" => crate::mesh::create_cube_document(&self.device, &self.viewport.node_bind_group_layout, 2.0),
            "Plane" => crate::mesh::create_plane_document(&self.device, &self.viewport.node_bind_group_layout, 2.5),
            _ => crate::mesh::create_sphere_document(&self.device, &self.viewport.node_bind_group_layout, 1.5, 32, 32),
        };
        self.viewport.set_document(doc);
        log::info!("Switched geometry to {}", mode);
    }

    fn export_composite_texture(&self) {
        let path = "painted_texture.png";
        self.painter.export_png(&self.device, &self.queue, path);
    }

    fn load_gltf_file(&mut self, path: &std::path::Path) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.gltf_rx = Some(rx);
        self.is_loading_gltf = true;

        let path = path.to_path_buf();
        let device = self.device.clone();
        let layout = self.viewport.node_bind_group_layout.clone();
        let window = self.window.clone();

        std::thread::spawn(move || {
            let res = crate::mesh::load_gltf(&device, &layout, &path)
                .map(|doc| (doc, path.file_name().unwrap_or_default().to_string_lossy().to_string()));
            let _ = tx.send(res);
            window.request_redraw();
        });
    }

    fn focus_camera_on_model(&mut self) {
        if let Some((min, max)) = self.viewport.document.compute_bounds() {
            let center = (min + max) * 0.5;
            let size = max - min;
            let max_dim = size.x.max(size.y).max(size.z);
            
            self.viewport.camera.target = center;
            self.viewport.camera.distance = (max_dim * 1.5).max(1.0);
            log::info!("Focused camera at center: {:?}, distance: {}", center, self.viewport.camera.distance);
        }
    }

    pub fn update(&mut self) {
        if self.needs_paint {
            self.paint_at_cursor();
            self.needs_paint = false;
        }

        if let Some(ref rx) = self.gltf_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_loading_gltf = false;
                self.gltf_rx = None;
                match res {
                    Ok((doc, filename)) => {
                        self.viewport.set_document(doc);
                        self.current_mesh_type = filename;
                        self.focus_camera_on_model();
                        log::info!("Successfully loaded glTF model asynchronously");
                    }
                    Err(e) => {
                        log::error!("Failed to load glTF model: {}", e);
                    }
                }
            }
        }
    }

    #[allow(deprecated)]
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        // 1. Begin Egui frame
        let egui_input = self.egui_state.take_egui_input(&*self.window);
        self.egui_ctx.begin_pass(egui_input);

        let mut export_requested = false;
        let mut clear_requested = false;
        let mut geometry_to_switch = None;
        let mut gltf_to_load = None;

        // Top Menu Bar
        egui::Panel::top("top_menu").show(&self.egui_ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open glTF Model...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("glTF Model", &["gltf", "glb"])
                            .pick_file()
                        {
                            gltf_to_load = Some(path);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Clear Canvas").clicked() {
                        clear_requested = true;
                        ui.close();
                    }
                    if ui.button("Export Composed Texture (PNG)").clicked() {
                        export_requested = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });

                ui.separator();
                ui.label("Model:");
                let current_mesh = self.current_mesh_type.clone();
                egui::ComboBox::from_id_salt("mesh_select")
                    .selected_text(&current_mesh)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut self.current_mesh_type, "Sphere".to_string(), "Sphere").changed() {
                            geometry_to_switch = Some("Sphere");
                        }
                        if ui.selectable_value(&mut self.current_mesh_type, "Cube".to_string(), "Cube").changed() {
                            geometry_to_switch = Some("Cube");
                        }
                        if ui.selectable_value(&mut self.current_mesh_type, "Plane".to_string(), "Plane").changed() {
                            geometry_to_switch = Some("Plane");
                        }
                    });

                if self.viewport.document.scenes.len() > 1 {
                    ui.separator();
                    ui.label("Scene:");
                    let active_idx = self.viewport.document.active_scene_idx;
                    let scene_name = self.viewport.document.scenes[active_idx].name
                        .clone()
                        .unwrap_or_else(|| format!("Scene {}", active_idx));
                    egui::ComboBox::from_id_salt("scene_select")
                        .selected_text(&scene_name)
                        .show_ui(ui, |ui| {
                            for (idx, scene) in self.viewport.document.scenes.iter().enumerate() {
                                let name = scene.name.clone().unwrap_or_else(|| format!("Scene {}", idx));
                                if ui.selectable_value(&mut self.viewport.document.active_scene_idx, idx, name).changed() {
                                    log::info!("Switched active scene to index {}", idx);
                                }
                            }
                        });
                }
            });
        });

        // Bottom Asset Shelf
        egui::Panel::bottom("asset_shelf").resizable(true).min_size(60.0).show(&self.egui_ctx, |ui| {
            ui.heading("Asset Shelf");
            ui.horizontal(|ui| {
                ui.label("Assets will appear here...");
            });
        });

        // Left Panel (Toolbar)
        egui::Panel::left("left_toolbar").resizable(false).show(&self.egui_ctx, |ui| {
            ui.heading("Tools");
            ui.separator();

            let brush_btn = ui.selectable_label(
                self.active_tool == Tool::Brush,
                format!("{} Brush", egui_phosphor::regular::PAINT_BRUSH),
            );
            if brush_btn.clicked() {
                self.active_tool = Tool::Brush;
            }

            let eraser_btn = ui.selectable_label(
                self.active_tool == Tool::Eraser,
                format!("{} Eraser", egui_phosphor::regular::ERASER),
            );
            if eraser_btn.clicked() {
                self.active_tool = Tool::Eraser;
            }

            ui.separator();
            if ui.button("Clear All").clicked() {
                clear_requested = true;
            }
        });

        // Right Panel (Properties and Layers)
        egui::Panel::right("right_panel").default_size(280.0).show(&self.egui_ctx, |ui| {
            ui.heading("Settings");
            ui.separator();

            egui::CollapsingHeader::new("Brush Settings")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        ui.add(egui::Slider::new(&mut self.brush_size, 2.0..=300.0).text("px"));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Hardness:");
                        ui.add(egui::Slider::new(&mut self.brush_hardness, 0.0..=1.0));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        ui.add(egui::Slider::new(&mut self.brush_opacity, 0.0..=1.0));
                    });

                    ui.separator();
                    ui.label("Color:");
                    
                    let mut color_f32 = [
                        self.brush_color[0] as f32 / 255.0,
                        self.brush_color[1] as f32 / 255.0,
                        self.brush_color[2] as f32 / 255.0,
                        self.brush_color[3] as f32 / 255.0,
                    ];
                    
                    if ui.color_edit_button_rgba_unmultiplied(&mut color_f32).changed() {
                        self.brush_color = [
                            (color_f32[0] * 255.0) as u8,
                            (color_f32[1] * 255.0) as u8,
                            (color_f32[2] * 255.0) as u8,
                            (color_f32[3] * 255.0) as u8,
                        ];
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Layers")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.new_layer_name);
                        if ui.button("Add").clicked() && !self.new_layer_name.trim().is_empty() {
                            self.painter.add_layer(self.new_layer_name.trim().to_string(), &self.device, &self.queue);
                            self.new_layer_name.clear();
                        }
                    });

                    if ui.button("Add UV Grid Layer").clicked() {
                        self.painter.load_uv_grid_layer(&self.device, &self.queue);
                    }

                    ui.separator();

                    let layer_count = self.painter.layers.len();
                    let mut layer_to_delete = None;
                    
                    for idx in (0..layer_count).rev() {
                        let is_active = self.painter.active_layer_idx == idx;
                        ui.horizontal(|ui| {
                            if ui.selectable_label(is_active, &self.painter.layers[idx].name).clicked() {
                                self.painter.active_layer_idx = idx;
                            }

                            let mut visible = self.painter.layers[idx].visible;
                            if ui.checkbox(&mut visible, "").changed() {
                                self.painter.layers[idx].visible = visible;
                                self.painter.compose_layers(&self.device, &self.queue);
                            }

                            if layer_count > 1 {
                                if ui.button("🗑").clicked() {
                                    layer_to_delete = Some(idx);
                                }
                            }
                        });

                        if is_active {
                            ui.indent("layer_props", |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Blend:");
                                    let mut blend = self.painter.layers[idx].blend_mode;
                                    egui::ComboBox::from_id_salt(format!("blend_{}", idx))
                                        .selected_text(blend.to_str())
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_value(&mut blend, BlendMode::Normal, "Normal").changed() {
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Multiply, "Multiply").changed() {
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Add, "Add").changed() {
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                        });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    let mut op = self.painter.layers[idx].opacity;
                                    if ui.add(egui::Slider::new(&mut op, 0.0..=1.0)).changed() {
                                        self.painter.layers[idx].opacity = op;
                                        self.painter.compose_layers(&self.device, &self.queue);
                                    }
                                });
                            });
                        }
                        ui.separator();
                    }

                    if let Some(to_del) = layer_to_delete {
                        self.painter.delete_layer(to_del, &self.device, &self.queue);
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Model Hierarchy")
                .default_open(true)
                .show(ui, |ui| {
                    let doc = &self.viewport.document;
                    if doc.scenes.is_empty() {
                        ui.label("No scene loaded");
                    } else {
                        let active_scene = &doc.scenes[doc.active_scene_idx];
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for &root_idx in &active_scene.root_nodes {
                                draw_node_tree(ui, &doc.nodes, root_idx);
                            }
                        });
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Light Settings")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Light Angle:");
                        ui.add(egui::Slider::new(&mut self.viewport.light_angle, 0.0..=std::f32::consts::PI * 2.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Light Intensity:");
                        ui.add(egui::Slider::new(&mut self.viewport.light_intensity, 0.0..=5.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ambient Strength:");
                        ui.add(egui::Slider::new(&mut self.viewport.ambient_strength, 0.0..=1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("View Transform:");
                        let current_transform = self.viewport.view_transform;
                        egui::ComboBox::from_id_salt("view_transform_select")
                            .selected_text(match current_transform {
                                crate::viewport::ViewTransform::Standard => "Standard Linear",
                                crate::viewport::ViewTransform::AgX => "AgX",
                                crate::viewport::ViewTransform::ACES => "ACES",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::Standard, "Standard Linear");
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::AgX, "AgX");
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::ACES, "ACES");
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Exposure:");
                        ui.add(egui::Slider::new(&mut self.viewport.exposure, 0.1..=5.0));
                    });
                });
        });

        // Handle requested actions
        if clear_requested {
            self.painter.clear_all_layers(&self.device, &self.queue);
        }

        if export_requested {
            self.export_composite_texture();
        }

        if let Some(mesh_type) = geometry_to_switch {
            self.toggle_mesh(mesh_type);
        }

        if let Some(path) = gltf_to_load {
            self.load_gltf_file(&path);
        }

        if self.is_loading_gltf {
            egui::Window::new("Loading Model")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .movable(false)
                .title_bar(false)
                .frame(egui::Frame::window(&self.egui_ctx.global_style())
                    .fill(egui::Color32::from_black_alpha(200))
                    .inner_margin(25.0)
                    .corner_radius(12.0))
                .show(&self.egui_ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(50.0));
                        ui.add_space(15.0);
                        ui.label(egui::RichText::new("Loading glTF Model...")
                            .size(18.0)
                            .color(egui::Color32::WHITE)
                            .strong());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Reading and compiling model resources in the background")
                            .size(12.0)
                            .color(egui::Color32::LIGHT_GRAY));
                    });
                });
        }

        let egui_output = self.egui_ctx.end_pass();
        let paint_jobs = self.egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);

        for (id, image_delta) in &egui_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        // 1. Fetch swapchain texture and create view
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(SurfaceError::Timeout),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(SurfaceError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(SurfaceError::Lost),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(SurfaceError::Timeout),
            wgpu::CurrentSurfaceTexture::Validation => return Err(SurfaceError::Other("Validation error".into())),
        };
        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Main Render Encoder"),
        });

        // 2. Update egui buffers on GPU
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Update uniforms on GPU
        self.viewport.update_camera(&self.queue);
        self.viewport.update_node_transforms(&self.queue);

        // 3. Render 3D Viewport
        self.viewport.render(
            &mut encoder,
            &view,
            &self.depth_view,
            &self.painter.bind_group,
        );

        // 4. Render Egui UI
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            }).forget_lifetime();

            self.egui_renderer.render(
                &mut egui_pass,
                &paint_jobs,
                &screen_descriptor,
            );
        }

        // Submit the command buffer
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        for id in &egui_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}

fn draw_node_tree(ui: &mut egui::Ui, nodes: &[crate::mesh::Node], node_idx: usize) {
    if node_idx >= nodes.len() {
        return;
    }
    let node = &nodes[node_idx];
    let label = node.name.clone().unwrap_or_else(|| format!("Node {}", node_idx));
    
    let has_children = !node.children.is_empty();
    let has_mesh = node.mesh.is_some();
    
    if !has_children && !has_mesh {
        ui.horizontal(|ui| {
            ui.label(format!("📄 {}", label));
        });
    } else {
        egui::CollapsingHeader::new(format!("📁 {}", label))
            .default_open(true)
            .show(ui, |ui| {
                if let Some(ref mesh) = node.mesh {
                    draw_mesh_info(ui, mesh);
                }
                for &child_idx in &node.children {
                    draw_node_tree(ui, nodes, child_idx);
                }
            });
    }
}

fn draw_mesh_info(ui: &mut egui::Ui, mesh: &crate::mesh::Mesh) {
    egui::CollapsingHeader::new("📦 Mesh")
        .default_open(true)
        .show(ui, |ui| {
            for (idx, prim) in mesh.primitives.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("📐 Primitive {}", idx));
                    ui.label(
                        egui::RichText::new(format!(
                            "({} verts, {} indices)",
                            prim.vertices.len(),
                            prim.num_indices
                        ))
                        .weak()
                        .size(10.0),
                    );
                });
            }
        });
}
