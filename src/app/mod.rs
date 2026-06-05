mod ui;
mod actions;
mod types;
mod user_preferences;
mod architecture;

pub use types::{Tool, SurfaceError, LoadError};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use winit::event::WindowEvent;
use winit::keyboard::KeyCode;
use winit::window::Window;
use crate::painter::BlendMode;
use user_preferences::{UserPreferences, KEY_CHOICES, MOUSE_BUTTON_CHOICES, parse_key_code, parse_mouse_button};

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

    // Egui state
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    
    // WGPU instance, adapter & UV viewer window
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    pub uv_viewer: Option<UvViewerWindow>,

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

    preferences: UserPreferences,
    preferences_path: PathBuf,
    settings_feedback: Option<String>,

    pub app_state: architecture::state::AppState,
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

        let (preferences, preferences_path) = UserPreferences::load_or_default();
        let initial_layer_count = painter.layers.len();
        let initial_num_udims = viewport.document.num_udim_tiles;

        let state = Self {
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
            egui_ctx,
            egui_state,
            egui_renderer,
            instance,
            adapter,
            uv_viewer: None,
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

            preferences,
            preferences_path,
            settings_feedback: None,
            app_state: architecture::state::AppState {
                document: architecture::state::DocumentState {
                    active_layer_idx: 0,
                    layer_count: initial_layer_count,
                    current_mesh: "Sphere".to_string(),
                    num_udim_tiles: initial_num_udims,
                    new_layer_name: String::new(),
                },
                canvas: architecture::state::CanvasState {
                    brush_size: 25.0,
                    brush_color: [220, 50, 50, 255],
                    brush_hardness: 0.5,
                    brush_opacity: 1.0,
                },
                tool: architecture::state::ToolState {
                    active_tool: Tool::Brush,
                },
                ui: architecture::state::UiState {
                    show_uv_viewer: false,
                    uv_viewer_source: 0,
                    uv_viewer_size: 256.0,
                    show_uv_wireframe: true,
                    show_pressure_calibration: false,
                },
                history: architecture::state::HistoryState {
                    undo_len: 0,
                    redo_len: 0,
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
                },
                resources: architecture::state::ResourceState {
                    is_loading_gltf: false,
                    has_error: false,
                },
                input: architecture::state::InputSnapshot {
                    ctrl: false,
                    cmd: false,
                    shift: false,
                    alt: false,
                    orbit_modifier: false,
                    pan_modifier: false,
                    paint_button_down: false,
                    pan_button_down: false,
                    has_tablet_input: false,
                    pen_pressure: 1.0,
                    touchpad_pressure_stage: 0,
                    last_mouse_pos: winit::dpi::PhysicalPosition::new(0.0, 0.0),
                    last_hit_uv: None,
                    last_hit_pos: None,
                },
            },
        };

        if !state.preferences_path.exists() {
            if let Err(e) = state.preferences.save_to(&state.preferences_path) {
                log::warn!("Failed to create default settings file: {}", e);
            }
        }

        Ok(state)
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

    pub fn calibrated_pressure(&self) -> f32 {
        let p = self.app_state.input.pen_pressure.clamp(0.0, 1.0);
        let min_start = self.preferences.pressure_curve_min_start.clamp(0.0, 1.0);
        let max_at = self.preferences.pressure_curve_max_at.clamp(min_start + 0.001, 1.0);
        ((p - min_start) / (max_at - min_start)).clamp(0.0, 1.0)
    }

    fn binding_matches_key(&self, binding: &user_preferences::KeyBinding, key: KeyCode) -> bool {
        let Some(expected) = parse_key_code(&binding.key) else {
            return false;
        };

        if expected != key {
            return false;
        }

        if binding.primary_mod {
            if !(self.app_state.input.ctrl || self.app_state.input.cmd) {
                return false;
            }
        }

        if binding.ctrl && !self.app_state.input.ctrl {
            return false;
        }
        if binding.cmd && !self.app_state.input.cmd {
            return false;
        }
        if binding.alt && !self.app_state.input.alt {
            return false;
        }
        if binding.shift && !self.app_state.input.shift {
            return false;
        }

        true
    }

    fn binding_matches_mouse(&self, binding: &user_preferences::MouseBinding, button: winit::event::MouseButton) -> bool {
        parse_mouse_button(&binding.button).map_or(false, |expected| expected == button)
    }

    fn save_settings(&mut self) {
        log::debug!("Saving bindings to {}", self.preferences_path.display());
        match self.preferences.save_to(&self.preferences_path) {
            Ok(()) => {
                let feedback = format!("Saved settings to {}", self.preferences_path.display());
                log::debug!("Bindings saved successfully: orbit_mod={}, pan_mod={}, undo={}, redo={}",
                    self.preferences.bindings.orbit_modifier.key,
                    self.preferences.bindings.pan_modifier.key,
                    self.preferences.bindings.undo.key,
                    self.preferences.bindings.redo.key);
                self.settings_feedback = Some(feedback);
            }
            Err(e) => {
                self.settings_feedback = Some(format!("Failed to save settings: {}", e));
                log::error!("Failed to save bindings to {}: {}", self.preferences_path.display(), e);
            }
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        let egui_resp = self.egui_state.on_window_event(&*self.window, event);
        if egui_resp.consumed {
            return true;
        }
        let messages = architecture::input::normalize_window_event(self, event);
        let mut consumed = false;
        for message in messages {
            consumed |= architecture::reducer::dispatch(self, message);
        }

        self.sync_app_state_snapshot();
        consumed
    }

    pub(crate) fn commit_current_stroke(&mut self) {
        if let Some(stroke) = self.current_stroke.take() {
            let active = self.painter.active_layer_idx;
            if active < self.painter.layers.len() && !self.painter.layers[active].is_fill {
                self.push_undo_state();
                self.painter.layers[active].strokes.push(stroke);
            }
        }
        self.app_state.input.last_hit_uv = None;
        self.app_state.input.last_hit_pos = None;
        self.app_state.input.paint_button_down = false;
        self.app_state.input.pen_pressure = 1.0;
        if self.app_state.input.touchpad_pressure_stage <= 0 {
            self.app_state.input.has_tablet_input = false;
        }
    }

    fn sync_app_state_snapshot(&mut self) {
        self.app_state.document.active_layer_idx = self.painter.active_layer_idx;
        self.app_state.document.layer_count = self.painter.layers.len();
        self.app_state.document.num_udim_tiles = self.viewport.document.num_udim_tiles;

        self.app_state.history.undo_len = self.app_state.history.undo_stack.len();
        self.app_state.history.redo_len = self.app_state.history.redo_stack.len();

        self.app_state.resources.is_loading_gltf = self.is_loading_gltf;
        self.app_state.resources.has_error = self.error_details.is_some();

        self.app_state.input.pan_modifier = self.app_state.input.alt;
    }

    fn draw_key_binding_editor(ui: &mut egui::Ui, id: &str, binding: &mut user_preferences::KeyBinding) {
        ui.horizontal(|ui| {
            ui.label("Key");
            egui::ComboBox::from_id_salt(id)
                .selected_text(&binding.key)
                .show_ui(ui, |ui| {
                    for key in KEY_CHOICES {
                        ui.selectable_value(&mut binding.key, (*key).to_string(), *key);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut binding.primary_mod, "Primary Mod (Ctrl/Cmd)");
            ui.checkbox(&mut binding.ctrl, "Ctrl");
            ui.checkbox(&mut binding.cmd, "Cmd");
            ui.checkbox(&mut binding.alt, "Alt");
            ui.checkbox(&mut binding.shift, "Shift");
        });
    }

    fn draw_mouse_binding_editor(ui: &mut egui::Ui, id: &str, binding: &mut user_preferences::MouseBinding) {
        ui.horizontal(|ui| {
            ui.label("Button");
            egui::ComboBox::from_id_salt(id)
                .selected_text(&binding.button)
                .show_ui(ui, |ui| {
                    for button in MOUSE_BUTTON_CHOICES {
                        ui.selectable_value(&mut binding.button, (*button).to_string(), *button);
                    }
                });
        });
    }

    fn key_binding_signature(binding: &user_preferences::KeyBinding) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if binding.primary_mod {
            parts.push("PrimaryMod");
        }
        if binding.ctrl {
            parts.push("Ctrl");
        }
        if binding.cmd {
            parts.push("Cmd");
        }
        if binding.alt {
            parts.push("Alt");
        }
        if binding.shift {
            parts.push("Shift");
        }
        parts.push(&binding.key);
        parts.join("+")
    }

    fn binding_conflicts(bindings: &user_preferences::InputBindings) -> Vec<String> {
        let key_bindings = [
            ("Orbit Modifier", &bindings.orbit_modifier),
            ("Pan Modifier", &bindings.pan_modifier),
            ("Undo", &bindings.undo),
            ("Redo", &bindings.redo),
            ("Clear Canvas", &bindings.clear_canvas),
            ("Brush Size Down", &bindings.brush_size_down),
            ("Brush Size Up", &bindings.brush_size_up),
            ("Select Brush Tool", &bindings.tool_brush),
            ("Select Eraser Tool", &bindings.tool_eraser),
        ];

        let mouse_bindings = [
            ("Paint Button", &bindings.paint_button),
            ("Pan Button", &bindings.pan_button),
        ];

        let mut by_combo: BTreeMap<String, Vec<&str>> = BTreeMap::new();

        for (name, binding) in key_bindings {
            let key = format!("Key:{}", Self::key_binding_signature(binding));
            by_combo.entry(key).or_default().push(name);
        }

        for (name, binding) in mouse_bindings {
            let key = format!("Mouse:{}", binding.button);
            by_combo.entry(key).or_default().push(name);
        }

        let mut warnings = Vec::new();
        for (combo, actions) in by_combo {
            if actions.len() > 1 {
                warnings.push(format!(
                    "{} is assigned to multiple actions: {}",
                    combo,
                    actions.join(", ")
                ));
            }
        }

        warnings
    }

    pub fn update(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
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
                        self.app_state.document.current_mesh = filename;
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

        // Spawn/destroy the UV viewer window based on show_uv_viewer flag
        if self.app_state.ui.show_uv_viewer && self.uv_viewer.is_none() {
            let window = Arc::new(event_loop.create_window(
                Window::default_attributes()
                    .with_title("UV Viewer")
                    .with_inner_size(winit::dpi::LogicalSize::new(800, 600)),
            ).unwrap());
            
            // Create surface for the child window
            let surface = self.instance.create_surface(window.clone()).unwrap();
            
            let caps = surface.get_capabilities(&self.adapter);
            let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
            
            let size = window.inner_size();
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&self.device, &config);
            
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
                Some(self.device.limits().max_texture_dimension_2d as usize),
            );
            
            let mut egui_renderer = egui_wgpu::Renderer::new(
                &self.device,
                format,
                egui_wgpu::RendererOptions {
                    depth_stencil_format: None,
                    msaa_samples: 1,
                    ..Default::default()
                },
            );
            
            // Register composite and layer textures in the child window's egui renderer
            let mut composite_tex_ids = Vec::new();
            for view in &self.painter.composite_views {
                let id = egui_renderer.register_native_texture(&self.device, view, wgpu::FilterMode::Linear);
                composite_tex_ids.push(id);
            }
            
            let mut layer_tex_ids = Vec::new();
            for view in &self.painter.layer_views {
                let id = egui_renderer.register_native_texture(&self.device, view, wgpu::FilterMode::Linear);
                layer_tex_ids.push(id);
            }
            
            self.uv_viewer = Some(UvViewerWindow {
                window,
                surface,
                config,
                egui_ctx,
                egui_state,
                egui_renderer,
                composite_tex_ids,
                layer_tex_ids,
            });
            log::info!("Opened floatable UV Viewer window.");
        } else if !self.app_state.ui.show_uv_viewer && self.uv_viewer.is_some() {
            self.uv_viewer = None;
            log::info!("Closed floatable UV Viewer window.");
        }
    }

    #[allow(deprecated)]
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let layers_snapshot = self.painter.layers.clone();
        let active_idx_snapshot = self.painter.active_layer_idx;
        let mut undo_state_to_push: Option<architecture::state::UndoState> = None;
        let mut undo_requested = false;
        let mut redo_requested = false;

        let push_undo_snapshot = || {
            architecture::state::UndoState {
                layers: layers_snapshot.clone(),
                active_layer_idx: active_idx_snapshot,
            }
        };

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
        let mut save_bindings_requested = false;
        let mut reset_bindings_requested = false;

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

                ui.menu_button("Edit", |ui| {
                    let undo_enabled = !self.app_state.history.undo_stack.is_empty();
                    let undo_label = if undo_enabled { "Undo (Ctrl+Z)" } else { "Undo" };
                    if ui.add_enabled(undo_enabled, egui::Button::new(undo_label)).clicked() {
                        undo_requested = true;
                        ui.close();
                    }
                    
                    let redo_enabled = !self.app_state.history.redo_stack.is_empty();
                    let redo_label = if redo_enabled { "Redo (Ctrl+Y)" } else { "Redo" };
                    if ui.add_enabled(redo_enabled, egui::Button::new(redo_label)).clicked() {
                        redo_requested = true;
                        ui.close();
                    }
                });

                ui.menu_button("Window", |ui| {
                    if ui.selectable_label(self.app_state.ui.show_uv_viewer, "UV Viewer").clicked() {
                        self.app_state.ui.show_uv_viewer = !self.app_state.ui.show_uv_viewer;
                        ui.close();
                    }
                });

                ui.separator();
                if ui.selectable_label(self.app_state.ui.show_uv_viewer, "🗺 View UVs").clicked() {
                    self.app_state.ui.show_uv_viewer = !self.app_state.ui.show_uv_viewer;
                }

                ui.separator();
                ui.label("Model:");
                let current_mesh = self.app_state.document.current_mesh.clone();
                egui::ComboBox::from_id_salt("mesh_select")
                    .selected_text(&current_mesh)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut self.app_state.document.current_mesh, "Sphere".to_string(), "Sphere").changed() {
                            geometry_to_switch = Some("Sphere");
                        }
                        if ui.selectable_value(&mut self.app_state.document.current_mesh, "Cube".to_string(), "Cube").changed() {
                            geometry_to_switch = Some("Cube");
                        }
                        if ui.selectable_value(&mut self.app_state.document.current_mesh, "Plane".to_string(), "Plane").changed() {
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
                self.app_state.tool.active_tool == Tool::Brush,
                format!("{} Brush", egui_phosphor::regular::PAINT_BRUSH),
            );
            if brush_btn.clicked() {
                self.app_state.tool.active_tool = Tool::Brush;
            }

            let eraser_btn = ui.selectable_label(
                self.app_state.tool.active_tool == Tool::Eraser,
                format!("{} Eraser", egui_phosphor::regular::ERASER),
            );
            if eraser_btn.clicked() {
                self.app_state.tool.active_tool = Tool::Eraser;
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
                        ui.add(egui::Slider::new(&mut self.app_state.canvas.brush_size, 2.0..=300.0).text("px"));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Hardness:");
                        ui.add(egui::Slider::new(&mut self.app_state.canvas.brush_hardness, 0.0..=1.0));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        ui.add(egui::Slider::new(&mut self.app_state.canvas.brush_opacity, 0.0..=1.0));
                    });

                    ui.separator();
                    if ui.button("Calibrate Pressure…").clicked() {
                        self.app_state.ui.show_pressure_calibration = true;
                    }

                    ui.separator();
                    ui.label("Color:");
                    
                    let mut color_f32 = [
                        self.app_state.canvas.brush_color[0] as f32 / 255.0,
                        self.app_state.canvas.brush_color[1] as f32 / 255.0,
                        self.app_state.canvas.brush_color[2] as f32 / 255.0,
                        self.app_state.canvas.brush_color[3] as f32 / 255.0,
                    ];
                    
                    if ui.color_edit_button_rgba_unmultiplied(&mut color_f32).changed() {
                        self.app_state.canvas.brush_color = [
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
                        ui.text_edit_singleline(&mut self.app_state.document.new_layer_name);
                        if ui.button("Add").clicked() && !self.app_state.document.new_layer_name.trim().is_empty() {
                            undo_state_to_push = Some(push_undo_snapshot());
                            self.painter.add_paint_layer(self.app_state.document.new_layer_name.trim().to_string(), &self.device, &self.queue);
                            self.app_state.document.new_layer_name.clear();
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Add UV Grid Layer").clicked() {
                            undo_state_to_push = Some(push_undo_snapshot());
                            self.painter.load_uv_grid_layer(&self.device, &self.queue);
                        }
                        if ui.button("Add UV Checker Layer").clicked() {
                            undo_state_to_push = Some(push_undo_snapshot());
                            self.painter.load_uv_checker_layer(&self.device, &self.queue);
                        }
                    });

                    if ui.button("✨ Add Fill Layer").clicked() {
                        undo_state_to_push = Some(push_undo_snapshot());
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
                                undo_state_to_push = Some(push_undo_snapshot());
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
                                                undo_state_to_push = Some(push_undo_snapshot());
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Multiply, "Multiply").changed() {
                                                undo_state_to_push = Some(push_undo_snapshot());
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Add, "Add").changed() {
                                                undo_state_to_push = Some(push_undo_snapshot());
                                                self.painter.layers[idx].blend_mode = blend;
                                                self.painter.compose_layers(&self.device, &self.queue);
                                            }
                                        });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    let mut op = self.painter.layers[idx].opacity;
                                    let response = ui.add(egui::Slider::new(&mut op, 0.0..=1.0));
                                    if response.drag_started() {
                                        undo_state_to_push = Some(push_undo_snapshot());
                                    }
                                    if response.changed() {
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
                                        let response = ui.color_edit_button_rgba_unmultiplied(&mut c);
                                        if response.drag_started() {
                                            undo_state_to_push = Some(push_undo_snapshot());
                                        }
                                        if response.changed() {
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
                                        let response = ui.color_edit_button_rgba_unmultiplied(&mut c);
                                        if response.drag_started() {
                                            undo_state_to_push = Some(push_undo_snapshot());
                                        }
                                        if response.changed() {
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
                                        let response = ui.add(egui::Slider::new(&mut scale, 0.5..=50.0));
                                        if response.drag_started() {
                                            undo_state_to_push = Some(push_undo_snapshot());
                                        }
                                        if response.changed() {
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
                                            undo_state_to_push = Some(push_undo_snapshot());
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

            ui.separator();
            egui::CollapsingHeader::new("Bindings")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Customize keys and mouse buttons for actions.");
            ui.small(format!("Settings file: {}", self.preferences_path.display()));
                    ui.separator();
                    ui.label(egui::RichText::new("Navigation").strong());
                    ui.group(|ui| {
                        ui.label("Orbit Modifier");
                        Self::draw_key_binding_editor(ui, "bind_orbit", &mut self.preferences.bindings.orbit_modifier);
                    });
                    ui.group(|ui| {
                        ui.label("Pan Modifier");
                        Self::draw_key_binding_editor(ui, "bind_pan_mod", &mut self.preferences.bindings.pan_modifier);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Edit Actions").strong());
                    ui.group(|ui| {
                        ui.label("Undo");
                        Self::draw_key_binding_editor(ui, "bind_undo", &mut self.preferences.bindings.undo);
                    });
                    ui.group(|ui| {
                        ui.label("Redo");
                        Self::draw_key_binding_editor(ui, "bind_redo", &mut self.preferences.bindings.redo);
                    });
                    ui.group(|ui| {
                        ui.label("Clear Canvas");
                        Self::draw_key_binding_editor(ui, "bind_clear", &mut self.preferences.bindings.clear_canvas);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Brush").strong());
                    ui.group(|ui| {
                        ui.label("Brush Size Down");
                        Self::draw_key_binding_editor(ui, "bind_size_down", &mut self.preferences.bindings.brush_size_down);
                    });
                    ui.group(|ui| {
                        ui.label("Brush Size Up");
                        Self::draw_key_binding_editor(ui, "bind_size_up", &mut self.preferences.bindings.brush_size_up);
                    });
                    ui.group(|ui| {
                        ui.label("Select Brush Tool");
                        Self::draw_key_binding_editor(ui, "bind_tool_brush", &mut self.preferences.bindings.tool_brush);
                    });
                    ui.group(|ui| {
                        ui.label("Select Eraser Tool");
                        Self::draw_key_binding_editor(ui, "bind_tool_eraser", &mut self.preferences.bindings.tool_eraser);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Mouse Buttons").strong());
                    ui.group(|ui| {
                        ui.label("Paint Button");
                        Self::draw_mouse_binding_editor(ui, "bind_paint_btn", &mut self.preferences.bindings.paint_button);
                    });
                    ui.group(|ui| {
                        ui.label("Pan Button");
                        Self::draw_mouse_binding_editor(ui, "bind_pan_btn", &mut self.preferences.bindings.pan_button);
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Save Bindings").clicked() {
                            save_bindings_requested = true;
                        }
                        if ui.button("Reset Defaults").clicked() {
                            reset_bindings_requested = true;
                        }
                    });

                    let conflicts = Self::binding_conflicts(&self.preferences.bindings);
                    if !conflicts.is_empty() {
                        ui.add_space(6.0);
                        ui.colored_label(egui::Color32::from_rgb(255, 180, 80), "Binding conflicts detected:");
                        for conflict in conflicts {
                            ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("- {}", conflict));
                        }
                    }

                    if let Some(msg) = &self.settings_feedback {
                        ui.label(egui::RichText::new(msg).small());
                    }
                });
            });
        });

        if reset_bindings_requested {
            self.preferences.bindings = user_preferences::InputBindings::default();
            self.save_settings();
        } else if save_bindings_requested {
            self.save_settings();
        }

        if self.app_state.ui.show_pressure_calibration {
            let mut is_open = self.app_state.ui.show_pressure_calibration;
            let egui_ctx = self.egui_ctx.clone();
            egui::Window::new("Pressure Calibration")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut is_open)
                .show(&egui_ctx, |ui| {
                    ui.label("Adjust pressure response curve");
                    ui.add_space(4.0);

                    let mut min_start = self.preferences.pressure_curve_min_start;
                    let mut max_at = self.preferences.pressure_curve_max_at;

                    ui.horizontal(|ui| {
                        ui.label("Min start");
                        ui.add(egui::Slider::new(&mut min_start, 0.0..=1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max at");
                        ui.add(egui::Slider::new(&mut max_at, 0.0..=1.0));
                    });

                    if min_start >= max_at {
                        if min_start == self.preferences.pressure_curve_min_start {
                            max_at = (min_start + 0.001).min(1.0);
                        } else {
                            min_start = (max_at - 0.001).max(0.0);
                        }
                    }
                    self.preferences.pressure_curve_min_start = min_start;
                    self.preferences.pressure_curve_max_at = max_at;

                    let calibrated = self.calibrated_pressure();
                    ui.separator();
                    ui.label(format!(
                        "Current pressure: {:.3} (mapped: {:.3})",
                        self.app_state.input.pen_pressure,
                        calibrated
                    ));
                    if self.app_state.input.touchpad_pressure_stage > 0 {
                        ui.label(format!("Force click stage: {}", self.app_state.input.touchpad_pressure_stage));
                    }

                    let graph_size = egui::vec2(320.0, 140.0);
                    let (rect, _) = ui.allocate_exact_size(graph_size, egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    let bg = egui::Color32::from_gray(20);
                    let stroke = egui::Stroke::new(1.0, egui::Color32::GRAY);
                    painter.rect_filled(rect, 4.0, bg);
                    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);

                    let to_screen = |x: f32, y: f32| -> egui::Pos2 {
                        egui::pos2(
                            egui::lerp(rect.left()..=rect.right(), x.clamp(0.0, 1.0)),
                            egui::lerp(rect.bottom()..=rect.top(), y.clamp(0.0, 1.0)),
                        )
                    };

                    let line_color = egui::Color32::from_rgb(120, 200, 255);
                    let curve = vec![
                        to_screen(0.0, 0.0),
                        to_screen(self.preferences.pressure_curve_min_start, 0.0),
                        to_screen(self.preferences.pressure_curve_max_at, 1.0),
                        to_screen(1.0, 1.0),
                    ];
                    painter.add(egui::Shape::line(curve, egui::Stroke::new(2.0, line_color)));

                    let marker = to_screen(self.app_state.input.pen_pressure, calibrated);
                    painter.circle_filled(marker, 4.0, egui::Color32::YELLOW);
                });
            self.app_state.ui.show_pressure_calibration = is_open;
        }

        if let Some(undo_state) = undo_state_to_push {
            self.app_state.history.redo_stack.clear();
            self.app_state.history.undo_stack.push(undo_state);
            if self.app_state.history.undo_stack.len() > 50 {
                self.app_state.history.undo_stack.remove(0);
            }
        }

        if undo_requested {
            self.undo();
        }

        if redo_requested {
            self.redo();
        }

        if clear_requested {
            self.push_undo_state();
            self.painter.clear_all_layers(&self.device, &self.queue);
        }

        if export_requested {
            self.export_composite_texture();
        }

        if let Some(mesh_type) = geometry_to_switch {
            self.push_undo_state();
            self.toggle_mesh(mesh_type);
        }

        if let Some(path) = gltf_to_load {
            self.load_gltf_file(&path);
        }

        if recompute_uv_requested {
            self.push_undo_state();
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

    pub fn render_uv_viewer(&mut self) -> Result<(), SurfaceError> {
        let viewer = match &mut self.uv_viewer {
            Some(v) => v,
            None => return Ok(()),
        };
        
        let egui_input = viewer.egui_state.take_egui_input(&*viewer.window);
        viewer.egui_ctx.begin_pass(egui_input);
        
        let num_tiles = self.viewport.document.num_udim_tiles.max(1) as usize;
        let show_uv_wireframe = self.app_state.ui.show_uv_wireframe;
        let active_nodes = self.viewport.document.get_active_nodes();
        
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&viewer.egui_ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Source:");
                
                let current_source = self.app_state.ui.uv_viewer_source;
                let source_name = if current_source == 0 {
                    "Composed Result".to_string()
                } else {
                    let idx = current_source - 1;
                    if idx < self.painter.layers.len() {
                        format!("Layer: {}", self.painter.layers[idx].name)
                    } else {
                        "Unknown Layer".to_string()
                    }
                };
                
                egui::ComboBox::from_id_salt("uv_viewer_source")
                    .selected_text(&source_name)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.app_state.ui.uv_viewer_source, 0, "Composed Result");
                        for idx in 0..self.painter.layers.len() {
                            ui.selectable_value(
                                &mut self.app_state.ui.uv_viewer_source,
                                idx + 1,
                                format!("Layer: {}", self.painter.layers[idx].name),
                            );
                        }
                    });
                
                ui.add_space(15.0);
                ui.checkbox(&mut self.app_state.ui.show_uv_wireframe, "Show UV Wireframe");
                    
                ui.add_space(20.0);
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.app_state.ui.uv_viewer_size, 64.0..=512.0).suffix("px"));
            });
            
            ui.separator();
            
            egui::ScrollArea::both().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for tile_idx in 0..num_tiles {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(format!("UDIM Tile {} (U: {}..{})", tile_idx, tile_idx, tile_idx + 1)).strong());
                            
                            let tex_id = if self.app_state.ui.uv_viewer_source == 0 {
                                if tile_idx < viewer.composite_tex_ids.len() {
                                    Some(viewer.composite_tex_ids[tile_idx])
                                } else {
                                    None
                                }
                            } else {
                                let layer_idx = self.app_state.ui.uv_viewer_source - 1;
                                let global_view_idx = layer_idx * crate::painter::MAX_UDIMS + tile_idx;
                                if global_view_idx < viewer.layer_tex_ids.len() {
                                    Some(viewer.layer_tex_ids[global_view_idx])
                                } else {
                                    None
                                }
                            };
                            
                            if let Some(id) = tex_id {
                                let img_size = self.app_state.ui.uv_viewer_size;
                                let response = ui.image((id, egui::vec2(img_size, img_size)));
                                let rect = response.rect;
                                
                                if show_uv_wireframe {
                                    let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120)); // semi-transparent white
                                    for (node, _) in &active_nodes {
                                        if let Some(ref mesh) = node.mesh {
                                            for primitive in &mesh.primitives {
                                                for chunk in primitive.indices.chunks_exact(3) {
                                                    let i0 = chunk[0] as usize;
                                                    let i1 = chunk[1] as usize;
                                                    let i2 = chunk[2] as usize;
                                                    
                                                    if i0 < primitive.vertices.len() && i1 < primitive.vertices.len() && i2 < primitive.vertices.len() {
                                                        let uv0 = primitive.vertices[i0].tex_coords;
                                                        let uv1 = primitive.vertices[i1].tex_coords;
                                                        let uv2 = primitive.vertices[i2].tex_coords;
                                                        
                                                        let local_u0 = uv0[0] - tile_idx as f32;
                                                        let local_u1 = uv1[0] - tile_idx as f32;
                                                        let local_u2 = uv2[0] - tile_idx as f32;
                                                        
                                                        let in_tile = (local_u0 >= 0.0 && local_u0 <= 1.0) ||
                                                                      (local_u1 >= 0.0 && local_u1 <= 1.0) ||
                                                                      (local_u2 >= 0.0 && local_u2 <= 1.0);
                                                                      
                                                        if in_tile {
                                                            let p0 = rect.min + egui::vec2(
                                                                local_u0.clamp(0.0, 1.0) * rect.width(),
                                                                (1.0 - uv0[1].clamp(0.0, 1.0)) * rect.height(),
                                                            );
                                                            let p1 = rect.min + egui::vec2(
                                                                local_u1.clamp(0.0, 1.0) * rect.width(),
                                                                (1.0 - uv1[1].clamp(0.0, 1.0)) * rect.height(),
                                                            );
                                                            let p2 = rect.min + egui::vec2(
                                                                local_u2.clamp(0.0, 1.0) * rect.width(),
                                                                (1.0 - uv2[1].clamp(0.0, 1.0)) * rect.height(),
                                                            );
                                                            
                                                            ui.painter().line_segment([p0, p1], stroke);
                                                            ui.painter().line_segment([p1, p2], stroke);
                                                            ui.painter().line_segment([p2, p0], stroke);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                ui.label("No texture view");
                            }
                        });
                        if tile_idx + 1 < num_tiles {
                            ui.add_space(10.0);
                        }
                    }
                });
            });
        });
        
        let egui_output = viewer.egui_ctx.end_pass();
        let paint_jobs = viewer.egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
        
        for (id, image_delta) in &egui_output.textures_delta.set {
            viewer.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }
        
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [viewer.config.width, viewer.config.height],
            pixels_per_point: viewer.window.scale_factor() as f32,
        };
        
        let surface_texture = match viewer.surface.get_current_texture() {
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
            label: Some("UV Viewer Render Encoder"),
        });
        
        viewer.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UV Viewer Egui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.15, g: 0.15, b: 0.15, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            }).forget_lifetime();
            
            viewer.egui_renderer.render(
                &mut egui_pass,
                &paint_jobs,
                &screen_descriptor,
            );
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        
        for id in &egui_output.textures_delta.free {
            viewer.egui_renderer.free_texture(id);
        }
        
        Ok(())
    }

    pub fn resize_uv_viewer(&mut self, width: u32, height: u32) {
        if let Some(ref mut viewer) = self.uv_viewer {
            if width > 0 && height > 0 {
                viewer.config.width = width;
                viewer.config.height = height;
                viewer.surface.configure(&self.device, &viewer.config);
            }
        }
    }

    pub fn push_undo_state(&mut self) {
        // Clear redo stack when a new action is performed
        self.app_state.history.redo_stack.clear();
        
        let undo_state = architecture::state::UndoState {
            layers: self.painter.layers.clone(),
            active_layer_idx: self.painter.active_layer_idx,
        };
        self.app_state.history.undo_stack.push(undo_state);
        
        if self.app_state.history.undo_stack.len() > 50 {
            self.app_state.history.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev_state) = self.app_state.history.undo_stack.pop() {
            let current_state = architecture::state::UndoState {
                layers: self.painter.layers.clone(),
                active_layer_idx: self.painter.active_layer_idx,
            };
            self.app_state.history.redo_stack.push(current_state);
            
            self.painter.layers = prev_state.layers;
            self.painter.active_layer_idx = prev_state.active_layer_idx;
            
            self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
            log::info!("Performed Undo. Undo stack size: {}, Redo stack size: {}", self.app_state.history.undo_stack.len(), self.app_state.history.redo_stack.len());
        } else {
            log::info!("Nothing to undo");
        }
    }

    pub fn redo(&mut self) {
        if let Some(next_state) = self.app_state.history.redo_stack.pop() {
            let current_state = architecture::state::UndoState {
                layers: self.painter.layers.clone(),
                active_layer_idx: self.painter.active_layer_idx,
            };
            self.app_state.history.undo_stack.push(current_state);
            
            self.painter.layers = next_state.layers;
            self.painter.active_layer_idx = next_state.active_layer_idx;
            
            self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
            log::info!("Performed Redo. Undo stack size: {}, Redo stack size: {}", self.app_state.history.undo_stack.len(), self.app_state.history.redo_stack.len());
        } else {
            log::info!("Nothing to redo");
        }
    }
}

pub struct UvViewerWindow {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    pub composite_tex_ids: Vec<egui::TextureId>,
    pub layer_tex_ids: Vec<egui::TextureId>,
}

