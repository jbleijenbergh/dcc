mod ui;
mod actions;
mod types;

pub use types::{Tool, SurfaceError, LoadError};

use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::Window;
use crate::painter::BlendMode;

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
    last_hit_pos: Option<glam::Vec3>,

    // Async loading state
    gltf_rx: Option<std::sync::mpsc::Receiver<Result<(crate::mesh::Document, String, Vec<Vec<crate::painter::PaintStroke>>), String>>>,
    gltf_loading_status: Option<Arc<std::sync::Mutex<String>>>,
    is_loading_gltf: bool,

    pub import_settings: crate::mesh::ImportSettings,
    current_stroke: Option<crate::painter::PaintStroke>,
    error_details: Option<LoadError>,
    error_time: Option<std::time::Instant>,
    loading_path: Option<std::path::PathBuf>,
    supported_present_modes: Vec<wgpu::PresentMode>,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        log::info!("Creating State with window size: {}x{}", size.width, size.height);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Failed to create WGPU surface: {e:?}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Failed to request WGPU adapter: {e:?}"))?;

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
            .map_err(|e| format!("Failed to create WGPU device: {e:?}"))?;
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

        let texture_bind_group_layout = crate::painter::create_bind_group_layout(&device);

        let mut painter = crate::painter::Painter::new(&device, &texture_bind_group_layout);
        painter.clear_all_layers(&device, &queue);

        let aspect = size.width as f32 / size.height as f32;
        let viewport = crate::viewport::Viewport::new(
            &device,
            surface_format,
            aspect,
            &texture_bind_group_layout,
        );
        viewport.update_node_transforms(&queue);

        let (depth_texture, depth_view) = crate::viewport::create_depth_texture(&device, &config, "depth_texture");

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

        Ok(Self {
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
            last_hit_pos: None,
            gltf_rx: None,
            gltf_loading_status: None,
            is_loading_gltf: false,
            import_settings: crate::mesh::ImportSettings {
                seams_option: crate::mesh::SeamsOption::GenerateMissing,
                margin_size: crate::mesh::MarginSize::Medium,
                island_orientation: crate::mesh::IslandOrientation::AlignWith3DMesh,
            },
            current_stroke: None,
            error_details: None,
            error_time: None,
            loading_path: None,
            supported_present_modes: surface_caps.present_modes,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            let (depth_texture, depth_view) = crate::viewport::create_depth_texture(&self.device, &self.config, "depth_texture");
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            self.viewport.camera.aspect = new_size.width as f32 / new_size.height as f32;

            log::info!("Resized to: {}x{}", new_size.width, new_size.height);
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::MouseInput { state: winit::event::ElementState::Released, button: winit::event::MouseButton::Left, .. } = event {
            if let Some(stroke) = self.current_stroke.take() {
                let active = self.painter.active_layer_idx;
                if active < self.painter.layers.len() && !self.painter.layers[active].is_fill {
                    self.painter.layers[active].strokes.push(stroke);
                    log::info!("Committed stroke to layer {}, total strokes: {}",
                        self.painter.layers[active].name,
                        self.painter.layers[active].strokes.len());
                }
            }
            self.last_hit_uv = None;
            self.last_hit_pos = None;
            self.is_left_clicked = false;
        }

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
                            self.last_hit_uv = None;
                            self.last_hit_pos = None;
                        } else {
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

                if self.egui_ctx.egui_wants_pointer_input() {
                    return false;
                }

                let is_navigating = self.is_space_pressed || self.is_alt_pressed;

                if self.is_left_clicked {
                    if is_navigating {
                        self.viewport.camera.yaw -= (dx * 0.005) as f32;
                        self.viewport.camera.pitch = (self.viewport.camera.pitch + (dy * 0.005) as f32)
                            .clamp(-std::f32::consts::FRAC_PI_2 + 0.05, std::f32::consts::FRAC_PI_2 - 0.05);
                        self.last_hit_uv = None;
                        self.last_hit_pos = None;
                    } else {
                        self.paint_at_cursor();
                    }
                    return true;
                }

                if self.is_right_clicked {
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

    pub fn update(&mut self) {
        if let Some(ref rx) = self.gltf_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_loading_gltf = false;
                self.gltf_rx = None;
                self.gltf_loading_status = None;
                let path = self.loading_path.take().unwrap_or_default();
                match res {
                    Ok((doc, filename, reprojected_strokes)) => {
                        self.viewport.set_document(doc);
                        self.viewport.update_node_transforms(&self.queue);
                        self.current_mesh_type = filename;
                        self.focus_camera_on_model();
                        self.error_details = None;
                        self.error_time = None;
                        
                        // Assign background reprojected strokes back to layers
                        for (layer_idx, strokes) in reprojected_strokes.into_iter().enumerate() {
                            if layer_idx < self.painter.layers.len() && !self.painter.layers[layer_idx].is_fill {
                                self.painter.layers[layer_idx].strokes = strokes;
                            }
                        }
                        
                        self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
                        log::info!("Successfully loaded glTF model — strokes reprojected in background");
                    }
                    Err(e) => {
                        log::error!("Failed to load glTF model: {}", e);
                        self.error_details = Some(LoadError { path, message: e.clone() });
                        self.error_time = Some(std::time::Instant::now());
                    }
                }
            }
        }
    }

    #[allow(deprecated)]
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let egui_input = self.egui_state.take_egui_input(&*self.window);
        self.egui_ctx.begin_pass(egui_input);

        let mut close_error = false;
        if let Some(ref err) = self.error_details {
            let can_dismiss = self.error_time.map_or(true, |t| t.elapsed().as_secs_f32() > 0.3);
            egui::Window::new("Error Loading Model")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(&self.egui_ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚠️").size(24.0));
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Failed to load glTF model").strong());
                                ui.label(egui::RichText::new(err.path.file_name().unwrap_or_default().to_string_lossy()).weak());
                            });
                        });
                        ui.add_space(8.0);
                        
                        ui.label(egui::RichText::new(&err.message).color(egui::Color32::from_rgb(255, 100, 100)));
                        ui.add_space(8.0);

                        let lower_msg = err.message.to_lowercase();
                        if lower_msg.contains("cannot find the path") || lower_msg.contains("os error 3") || lower_msg.contains("not found") {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("💡 Quick Suggestion:").strong().size(11.0));
                                ui.label(egui::RichText::new(
                                    "This glTF file references external assets (such as a separate .bin buffer or image textures) \
                                     that could not be found. Ensure all referenced files are in the same folder as the .gltf file."
                                ).size(10.5));
                            });
                            ui.add_space(8.0);
                        }

                        egui::CollapsingHeader::new("Technical Details")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(format!("File Path: {}", err.path.display())).weak().size(10.0));
                                ui.label(egui::RichText::new(format!("Raw Error: {}", err.message)).weak().size(10.0));
                            });

                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.add_enabled_ui(can_dismiss, |ui| {
                                if ui.button("OK").clicked() {
                                    close_error = true;
                                }
                            });
                        });
                    });
                });
        }
        if close_error {
            self.error_details = None;
            self.error_time = None;
        }

        let mut export_requested = false;
        let mut clear_requested = false;
        let mut geometry_to_switch = None;
        let mut gltf_to_load = None;
        let mut recompute_uv_requested = false;

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
                            for (idx, _scene) in self.viewport.document.scenes.iter().enumerate() {
                                let name = _scene.name.clone().unwrap_or_else(|| format!("Scene {}", idx));
                                if ui.selectable_value(&mut self.viewport.document.active_scene_idx, idx, name).changed() {
                                    log::info!("Switched active scene to index {}", idx);
                                }
                            }
                        });
                }
            });
        });

        egui::Panel::bottom("asset_shelf").resizable(true).min_size(60.0).show(&self.egui_ctx, |ui| {
            ui.heading("Asset Shelf");
            ui.horizontal(|ui| {
                ui.label("Assets will appear here...");
            });
        });

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

        egui::Panel::right("right_panel").default_size(280.0).show(&self.egui_ctx, |ui| {
            ui.heading("Settings");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
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
                            self.painter.add_paint_layer(self.new_layer_name.trim().to_string(), &self.device, &self.queue);
                            self.new_layer_name.clear();
                        }
                    });

                    if ui.button("Add UV Grid Layer").clicked() {
                        self.painter.load_uv_grid_layer(&self.device, &self.queue);
                    }

                    if ui.button("✨ Add Fill Layer").clicked() {
                        let name = format!("Fill {}", self.painter.layers.len() + 1);
                        self.painter.add_fill_layer(name, &self.device, &self.queue, &self.viewport.document);
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
                            let is_fill = self.painter.layers[idx].is_fill;
                            let mut fill_changed = false;
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

                                if is_fill {
                                    ui.separator();
                                    ui.label(egui::RichText::new("Fill Layer Settings").strong());

                                    ui.horizontal(|ui| {
                                        ui.label("Base Color:");
                                        let mut c = [
                                            self.painter.layers[idx].fill_color[0] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[1] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[2] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[3] as f32 / 255.0,
                                        ];
                                        if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                                            self.painter.layers[idx].fill_color = [
                                                (c[0] * 255.0) as u8, (c[1] * 255.0) as u8,
                                                (c[2] * 255.0) as u8, (c[3] * 255.0) as u8,
                                            ];
                                            fill_changed = true;
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Noise Color:");
                                        let mut c = [
                                            self.painter.layers[idx].fill_noise_color[0] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[1] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[2] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[3] as f32 / 255.0,
                                        ];
                                        if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                                            self.painter.layers[idx].fill_noise_color = [
                                                (c[0] * 255.0) as u8, (c[1] * 255.0) as u8,
                                                (c[2] * 255.0) as u8, (c[3] * 255.0) as u8,
                                            ];
                                            fill_changed = true;
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Noise Scale:");
                                        let mut scale = self.painter.layers[idx].fill_noise_scale;
                                        if ui.add(egui::Slider::new(&mut scale, 0.5..=50.0)).changed() {
                                            self.painter.layers[idx].fill_noise_scale = scale;
                                            fill_changed = true;
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Projection:");
                                        let mut mode = self.painter.layers[idx].fill_projection_mode;
                                        let prev_mode = mode;
                                        egui::ComboBox::from_id_salt(format!("proj_{}", idx))
                                            .selected_text(if mode == 1 { "Triplanar" } else { "UV" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut mode, 0u32, "UV");
                                                ui.selectable_value(&mut mode, 1u32, "Triplanar");
                                            });
                                        if mode != prev_mode {
                                            self.painter.layers[idx].fill_projection_mode = mode;
                                            fill_changed = true;
                                        }
                                    });
                                }
                            });

                            if is_fill && fill_changed {
                                let layer = &self.painter.layers[idx];
                                let base = [
                                    layer.fill_color[0] as f32 / 255.0, layer.fill_color[1] as f32 / 255.0,
                                    layer.fill_color[2] as f32 / 255.0, layer.fill_color[3] as f32 / 255.0,
                                ];
                                let noise = [
                                    layer.fill_noise_color[0] as f32 / 255.0, layer.fill_noise_color[1] as f32 / 255.0,
                                    layer.fill_noise_color[2] as f32 / 255.0, layer.fill_noise_color[3] as f32 / 255.0,
                                ];
                                let scale = layer.fill_noise_scale;
                                let proj = layer.fill_projection_mode;
                                self.painter.render_fill_layer(
                                    &self.device, &self.queue, idx, base, noise, scale, proj,
                                    &self.viewport.document,
                                );
                                self.painter.compose_layers(&self.device, &self.queue);
                            }
                        }
                        ui.separator();
                    }

                    if let Some(to_del) = layer_to_delete {
                        self.painter.delete_layer(to_del, &self.device, &self.queue);
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("UV Settings")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Seams:");
                        egui::ComboBox::from_id_salt("seams_select")
                            .selected_text(match self.import_settings.seams_option {
                                crate::mesh::SeamsOption::GenerateMissing => "Generate Missing",
                                crate::mesh::SeamsOption::RecomputeAll => "Recompute All",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.import_settings.seams_option,
                                    crate::mesh::SeamsOption::GenerateMissing,
                                    "Generate Missing",
                                );
                                ui.selectable_value(
                                    &mut self.import_settings.seams_option,
                                    crate::mesh::SeamsOption::RecomputeAll,
                                    "Recompute All",
                                );
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Margin:");
                        egui::ComboBox::from_id_salt("margin_select")
                            .selected_text(match self.import_settings.margin_size {
                                crate::mesh::MarginSize::Small => "Small",
                                crate::mesh::MarginSize::Medium => "Medium",
                                crate::mesh::MarginSize::Large => "Large",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.import_settings.margin_size,
                                    crate::mesh::MarginSize::Small,
                                    "Small",
                                );
                                ui.selectable_value(
                                    &mut self.import_settings.margin_size,
                                    crate::mesh::MarginSize::Medium,
                                    "Medium",
                                );
                                ui.selectable_value(
                                    &mut self.import_settings.margin_size,
                                    crate::mesh::MarginSize::Large,
                                    "Large",
                                );
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Orientation:");
                        egui::ComboBox::from_id_salt("orientation_select")
                            .selected_text(match self.import_settings.island_orientation {
                                crate::mesh::IslandOrientation::AlignWith3DMesh => "Align with 3D Mesh",
                                crate::mesh::IslandOrientation::Unconstrained => "Unconstrained",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.import_settings.island_orientation,
                                    crate::mesh::IslandOrientation::AlignWith3DMesh,
                                    "Align with 3D Mesh",
                                );
                                ui.selectable_value(
                                    &mut self.import_settings.island_orientation,
                                    crate::mesh::IslandOrientation::Unconstrained,
                                    "Unconstrained",
                                );
                            });
                    });

                    ui.add_space(4.0);
                    if ui.button("🔄 Recompute UVs & Reproject Strokes").clicked() {
                        recompute_uv_requested = true;
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
                                self::ui::draw_node_tree(ui, &doc.nodes, root_idx, &doc.materials);
                            }
                        });
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Materials")
                .default_open(true)
                .show(ui, |ui| {
                    let doc = &self.viewport.document;
                    if doc.materials.is_empty() {
                        ui.label(egui::RichText::new("No materials in model").weak().italics());
                    } else {
                        let mut unique_materials = Vec::new();
                        for (idx, mat) in doc.materials.iter().enumerate() {
                            let is_duplicate = if let Some(gltf_id) = mat.gltf_index {
                                unique_materials.iter().any(|(_, m): &(usize, &crate::mesh::MaterialInfo)| m.gltf_index == Some(gltf_id))
                            } else {
                                unique_materials.iter().any(|(_, m): &(usize, &crate::mesh::MaterialInfo)| m.name == mat.name && m.gltf_index.is_none())
                            };
                            if !is_duplicate {
                                unique_materials.push((idx, mat));
                            }
                        }

                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (idx, mat) in unique_materials {
                                self::ui::draw_material_details(ui, idx, mat);
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

            ui.separator();
            egui::CollapsingHeader::new("Display Info")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Color Format:");
                        ui.label(egui::RichText::new(format!("{:?}", self.config.format)).strong());
                    });

                    let refresh_rate = self.window.current_monitor()
                        .and_then(|m| m.refresh_rate_millihertz())
                        .map(|mhz| format!("{:.1} Hz", mhz as f32 / 1000.0))
                        .unwrap_or_else(|| "Unknown".to_string());
                    ui.horizontal(|ui| {
                        ui.label("Refresh Rate:");
                        ui.label(egui::RichText::new(refresh_rate).strong());
                    });

                    ui.horizontal(|ui| {
                        ui.label("VSync / Present Mode:");
                        let mut present_mode = self.config.present_mode;
                        let prev_mode = present_mode;
                        egui::ComboBox::from_id_salt("present_mode_select")
                            .selected_text(match present_mode {
                                wgpu::PresentMode::Fifo => "VSync On (Fifo)",
                                wgpu::PresentMode::Immediate => "VSync Off (Immediate)",
                                wgpu::PresentMode::Mailbox => "VSync Off (Mailbox)",
                                wgpu::PresentMode::AutoVsync => "Auto VSync",
                                wgpu::PresentMode::AutoNoVsync => "Auto VSync Off",
                                _ => "Other",
                            })
                            .show_ui(ui, |ui| {
                                if self.supported_present_modes.contains(&wgpu::PresentMode::Fifo) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Fifo, "VSync On (Fifo)");
                                }
                                if self.supported_present_modes.contains(&wgpu::PresentMode::Immediate) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Immediate, "VSync Off (Immediate)");
                                }
                                if self.supported_present_modes.contains(&wgpu::PresentMode::Mailbox) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Mailbox, "VSync Off (Mailbox)");
                                }
                                if self.supported_present_modes.contains(&wgpu::PresentMode::AutoVsync) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::AutoVsync, "Auto VSync");
                                }
                                if self.supported_present_modes.contains(&wgpu::PresentMode::AutoNoVsync) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::AutoNoVsync, "Auto VSync Off");
                                }
                            });
                        if present_mode != prev_mode {
                            self.config.present_mode = present_mode;
                            self.surface.configure(&self.device, &self.config);
                            log::info!("Present mode switched to {:?}", present_mode);
                        }
                    });
                });
            });
        });

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

        if recompute_uv_requested {
            self.recompute_and_reproject();
        }

        if self.is_loading_gltf {
            let status_msg = self.gltf_loading_status.as_ref()
                .and_then(|status| status.lock().ok())
                .map(|g| g.clone())
                .unwrap_or_else(|| "Reading and compiling model resources in the background".to_string());

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
                        ui.label(egui::RichText::new(status_msg)
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

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        self.viewport.update_camera(&self.queue);

        self.viewport.render(
            &mut encoder,
            &view,
            &self.depth_view,
            &self.painter.bind_group,
        );

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

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        for id in &egui_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}
