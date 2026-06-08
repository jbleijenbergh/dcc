mod ui;
mod actions;
mod types;
mod user_preferences;
mod app_state;
mod tools;
mod ecs;
pub(crate) mod input;
mod ui_panels;

pub use types::{Tool, SurfaceError, LoadError};

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::Window;
use user_preferences::UserPreferences;

#[derive(Default)]
pub(crate) struct InteractionState {
    pub stroke_in_progress: Option<crate::painter::PaintStroke>,
    pub last_hit_uv: Option<glam::Vec2>,
    pub last_hit_pos: Option<glam::Vec3>,
}

/// Host-side ECS coordinator state.
///
/// Keeps ECS runtime together with frame lifecycle flags to reduce `State`
/// ownership surface while preserving existing call patterns.
pub(crate) struct HostEcsRuntime {
    runtime: ecs::EcsRuntime,
    ui_frame_ops: ecs::PendingUiFrameOpsResource,
}

impl HostEcsRuntime {
    fn new(runtime: ecs::EcsRuntime) -> Self {
        Self {
            runtime,
            ui_frame_ops: ecs::PendingUiFrameOpsResource::default(),
        }
    }
}

impl Deref for HostEcsRuntime {
    type Target = ecs::EcsRuntime;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl DerefMut for HostEcsRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime
    }
}

/// Host-side UV window runtime state.
#[derive(Default)]
pub(crate) struct UvUiRuntime {
    viewer: Option<UvViewerWindow>,
    frame_begun: bool,
}

type GltfLoadResult =
    Result<(crate::mesh::Document, String, Vec<Vec<crate::painter::PaintStroke>>), String>;

#[derive(Default)]
pub(crate) struct AssetLoadCoordinator {
    gltf_rx: Option<std::sync::mpsc::Receiver<GltfLoadResult>>,
    gltf_loading_status: Option<Arc<std::sync::Mutex<String>>>,
    loading_path: Option<std::path::PathBuf>,
}

pub(crate) struct RenderHostCoordinator {
    supported_present_modes: Vec<wgpu::PresentMode>,
}

impl RenderHostCoordinator {
    fn new(supported_present_modes: Vec<wgpu::PresentMode>) -> Self {
        Self { supported_present_modes }
    }

    fn supports_present_mode(&self, mode: wgpu::PresentMode) -> bool {
        self.supported_present_modes.contains(&mode)
    }

    fn handle_render_error(
        &self,
        ecs_runtime: &mut ecs::EcsRuntime,
        surface: ecs::events::RenderSurfaceKind,
        err: SurfaceError,
    ) -> Result<(), SurfaceError> {
        match err {
            SurfaceError::Lost => ecs_runtime.send_render_failure_event(
                ecs::events::RenderFailureEvent {
                    surface,
                    kind: ecs::events::RenderFailureKind::Lost,
                },
            ),
            SurfaceError::Outdated => ecs_runtime.send_render_failure_event(
                ecs::events::RenderFailureEvent {
                    surface,
                    kind: ecs::events::RenderFailureKind::Outdated,
                },
            ),
            SurfaceError::Timeout => {}
            SurfaceError::Other(e) => return Err(SurfaceError::Other(e)),
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct SurfaceHostCoordinator;

impl SurfaceHostCoordinator {
    fn apply_pending_surface_ops(
        &self,
        surface_ops: ecs::PendingSurfaceOpsResource,
        size: &mut winit::dpi::PhysicalSize<u32>,
        config: &mut wgpu::SurfaceConfiguration,
        surface: &wgpu::Surface<'static>,
        device: &Arc<wgpu::Device>,
        depth_texture: &mut wgpu::Texture,
        depth_view: &mut wgpu::TextureView,
        viewport: &mut crate::viewport::Viewport,
        uv_viewer: &mut Option<UvViewerWindow>,
        ecs_runtime: &mut HostEcsRuntime,
    ) {
        if let Some((width, height)) = surface_ops.main_resize {
            self.resize_main_surface(
                winit::dpi::PhysicalSize::new(width, height),
                size,
                config,
                surface,
                device,
                depth_texture,
                depth_view,
                viewport,
            );
        }
        if let Some((width, height)) = surface_ops.uv_resize {
            self.resize_uv_surface(width, height, uv_viewer, device, ecs_runtime);
        }
    }

    fn resize_main_surface(
        &self,
        new_size: winit::dpi::PhysicalSize<u32>,
        size: &mut winit::dpi::PhysicalSize<u32>,
        config: &mut wgpu::SurfaceConfiguration,
        surface: &wgpu::Surface<'static>,
        device: &Arc<wgpu::Device>,
        depth_texture: &mut wgpu::Texture,
        depth_view: &mut wgpu::TextureView,
        viewport: &mut crate::viewport::Viewport,
    ) {
        if new_size.width > 0 && new_size.height > 0 {
            *size = new_size;
            config.width = new_size.width;
            config.height = new_size.height;
            surface.configure(device, config);

            let (new_depth_texture, new_depth_view) =
                crate::viewport::create_depth_texture(device, config, "depth_texture");
            *depth_texture = new_depth_texture;
            *depth_view = new_depth_view;

            viewport.camera.aspect = new_size.width as f32 / new_size.height as f32;

            log::info!("Resized to: {}x{}", new_size.width, new_size.height);
        }
    }

    fn resize_uv_surface(
        &self,
        width: u32,
        height: u32,
        uv_viewer: &mut Option<UvViewerWindow>,
        device: &Arc<wgpu::Device>,
        ecs_runtime: &mut HostEcsRuntime,
    ) {
        if let Some(ref mut viewer) = uv_viewer {
            if width > 0 && height > 0 {
                viewer.config.width = width;
                viewer.config.height = height;
                viewer.surface.configure(device, &viewer.config);
                ecs_runtime.set_uv_surface_size(width, height);
            }
        }
    }

    fn queue_main_window_resize(
        &self,
        ecs_runtime: &mut HostEcsRuntime,
        width: u32,
        height: u32,
    ) {
        ecs_runtime.send_window_surface_event(ecs::events::WindowSurfaceEvent::MainWindowResized {
            width,
            height,
        });
    }

    fn queue_uv_window_resize(
        &self,
        ecs_runtime: &mut HostEcsRuntime,
        width: u32,
        height: u32,
    ) {
        ecs_runtime.send_window_surface_event(ecs::events::WindowSurfaceEvent::UvWindowResized {
            width,
            height,
        });
    }
}

#[derive(Default)]
pub(crate) struct RenderSchedulingCoordinator;

impl RenderSchedulingCoordinator {
    fn queue_main_redraw(&self, ecs_runtime: &mut HostEcsRuntime) {
        ecs_runtime.send_redraw_event(ecs::events::RedrawEvent::MainSurface);
    }

    fn queue_uv_redraw(&self, ecs_runtime: &mut HostEcsRuntime) {
        ecs_runtime.send_redraw_event(ecs::events::RedrawEvent::UvSurface);
    }

    fn should_render_main_surface(&self, render_ops: &ecs::PendingRenderOpsResource) -> bool {
        render_ops.render_main_surface
            || render_ops.render_3d_viewport_pass
            || render_ops.render_paint_composite_pass
    }

    fn should_render_uv_surface(&self, render_ops: &ecs::PendingRenderOpsResource) -> bool {
        render_ops.render_uv_surface
    }
}

/// Host-side main window egui runtime state.
pub(crate) struct MainUiRuntime {
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    frame_begun: bool,
}

impl MainUiRuntime {
    fn new(
        egui_ctx: egui::Context,
        egui_state: egui_winit::State,
        egui_renderer: egui_wgpu::Renderer,
    ) -> Self {
        Self {
            egui_ctx,
            egui_state,
            egui_renderer,
            frame_begun: false,
        }
    }
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

    // Main window egui state
    main_ui: MainUiRuntime,
    
    // WGPU instance, adapter & UV viewer window
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    uv_ui: UvUiRuntime,

    // Async loading state
    asset_loader: AssetLoadCoordinator,
    render_host: RenderHostCoordinator,
    surface_host: SurfaceHostCoordinator,
    render_scheduler: RenderSchedulingCoordinator,

    pub import_settings: crate::mesh::ImportSettings,
    interaction: InteractionState,

    preferences: UserPreferences,
    preferences_path: PathBuf,
    ui_state: ui::TransientUiState,

    pub app_state: app_state::AppState,
    
    // ECS runtime
    pub(crate) ecs_runtime: HostEcsRuntime,
}

impl State {
    pub fn uv_viewer(&self) -> Option<&UvViewerWindow> {
        self.uv_ui.viewer.as_ref()
    }

    pub fn uv_viewer_mut(&mut self) -> Option<&mut UvViewerWindow> {
        self.uv_ui.viewer.as_mut()
    }

    pub fn clear_uv_viewer_window(&mut self) {
        self.uv_ui.viewer = None;
    }

    fn process_ecs_step(&mut self) {
        self.ecs_runtime.sync_domain_state_from(&self.app_state);
        self.ecs_runtime.tick();
        self.apply_pending_domain_host_ops();
    }

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

        let mut ecs_runtime = ecs::EcsRuntime::new();

        let app_state = {
            let mut state = app_state::AppState::new();
            state.document_mut().active_layer_idx = 0;
            state.document_mut().layer_count = initial_layer_count;
            state.document_mut().current_mesh = "Sphere".to_string();
            state.document_mut().num_udim_tiles = initial_num_udims;
            state.canvas_mut().brush_size = 25.0;
            state.canvas_mut().brush_color = [220, 50, 50, 255];
            state.canvas_mut().brush_hardness = 0.5;
            state.ui_mut().show_uv_wireframe = true;
            state
        };

        // Register resources into the ECS world
        ecs_runtime.register_domain_state(app_state.clone());
        ecs_runtime.register_interaction_state(ecs::InteractionStateResource::default());
        ecs_runtime.register_preferences(preferences.clone());
        ecs_runtime.register_main_ui_resource(egui_ctx.clone(), true, true);
        ecs_runtime.update_uv_ui_resource(None, false, false);
        ecs_runtime.register_gpu_context(
            instance.clone(),
            adapter.clone(),
            device.clone(),
            queue.clone(),
        );
        ecs_runtime.register_surface_registry(size.width, size.height);

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
            main_ui: MainUiRuntime::new(egui_ctx, egui_state, egui_renderer),
            instance,
            adapter,
            uv_ui: UvUiRuntime::default(),
            asset_loader: AssetLoadCoordinator::default(),
            render_host: RenderHostCoordinator::new(surface_caps.present_modes),
            surface_host: SurfaceHostCoordinator::default(),
            render_scheduler: RenderSchedulingCoordinator::default(),
            import_settings: crate::mesh::ImportSettings {
                seams_option: crate::mesh::SeamsOption::GenerateMissing,
                margin_size: crate::mesh::MarginSize::Medium,
                island_orientation: crate::mesh::IslandOrientation::AlignWith3DMesh,
            },
            interaction: InteractionState::default(),

            preferences,
            preferences_path,
            ui_state: ui::TransientUiState::default(),
            app_state,
            ecs_runtime: HostEcsRuntime::new(ecs_runtime),
        };

        if !state.preferences_path.exists() {
            if let Err(e) = state.preferences.save_to(&state.preferences_path) {
                log::warn!("Failed to create default settings file: {}", e);
            }
        }

        Ok(state)
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.surface_host.resize_main_surface(
            new_size,
            &mut self.size,
            &mut self.config,
            &self.surface,
            &self.device,
            &mut self.depth_texture,
            &mut self.depth_view,
            &mut self.viewport,
        );
    }

    pub fn calibrated_pressure(&self) -> f32 {
        let p = self.app_state.input().pen_pressure.clamp(0.0, 1.0);
        let min_start = self.preferences.pressure_curve_min_start.clamp(0.0, 1.0);
        let max_at = self.preferences.pressure_curve_max_at.clamp(min_start + 0.001, 1.0);
        ((p - min_start) / (max_at - min_start)).clamp(0.0, 1.0)
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
                self.ui_state.settings_feedback = Some(feedback);
            }
            Err(e) => {
                self.ui_state.settings_feedback = Some(format!("Failed to save settings: {}", e));
                log::error!("Failed to save bindings to {}: {}", self.preferences_path.display(), e);
            }
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        let egui_resp = self.main_ui.egui_state.on_window_event(&*self.window, event);
        if egui_resp.consumed {
            return true;
        }
        let events = input::normalize_window_event(self.app_state.input(), &self.preferences.bindings, event);
        let mut consumed = false;
        for event in events {
            self.emit_event(event);
            consumed = true;
        }

        self.sync_app_state_snapshot();
        consumed
    }

    pub(crate) fn commit_current_stroke(&mut self) {
        if let Some(stroke) = self.interaction.stroke_in_progress.take() {
            let active = self.painter.active_layer_idx;
            if active < self.painter.layers.len() && !self.painter.layers[active].is_fill {
                self.push_undo_state();
                self.painter.layers[active].strokes.push(stroke);
            }
        }
        self.interaction.last_hit_uv = None;
        self.interaction.last_hit_pos = None;
        self.app_state.input_mut().paint_button_down = false;
        self.app_state.input_mut().pen_pressure = 1.0;
        if self.app_state.input_mut().touchpad_pressure_stage <= 0 {
            self.app_state.input_mut().has_tablet_input = false;
        }
    }

    fn sync_app_state_snapshot(&mut self) {
        self.app_state.document_mut().active_layer_idx = self.painter.active_layer_idx;
        self.app_state.document_mut().layer_count = self.painter.layers.len();
        self.app_state.document_mut().num_udim_tiles = self.viewport.document.num_udim_tiles;

        self.app_state.history_mut().undo_len = self.app_state.history().undo_stack.len();
        self.app_state.history_mut().redo_len = self.app_state.history().redo_stack.len();

        self.app_state.input_mut().pan_modifier = self.app_state.input().alt;

        // Sync camera data
        let camera_mut = self.app_state.camera_mut();
        camera_mut.eye = self.viewport.camera.get_eye();
        camera_mut.target = self.viewport.camera.target;
        camera_mut.yaw = self.viewport.camera.yaw;
        camera_mut.pitch = self.viewport.camera.pitch;
        camera_mut.distance = self.viewport.camera.distance;
        camera_mut.fov = self.viewport.camera.fovy;
        camera_mut.aspect = self.viewport.camera.aspect;

        // Sync layer composition data
        let composition_mut = self.app_state.layer_composition_mut();
        composition_mut.visibilities = self.painter.layers.iter().map(|l| l.visible).collect();
        composition_mut.opacities = self.painter.layers.iter().map(|l| l.opacity).collect();
    }

    /// Emit an ECS event into the runtime's event queue.
    ///
    /// Runtime input and command routing is ECS-native.
    pub fn emit_event(&mut self, event: ecs::events::AppEvent) {
        self.ecs_runtime.send_event(event);
    }

    fn emit_ui_action(&mut self, action: ecs::events::UiActionEvent) {
        self.emit_event(ecs::events::AppEvent::Ui(action));
    }

pub fn update(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> Result<(), SurfaceError> {
        // Phase 1: ECS schedule tick + host adapter consumption.
        self.process_ecs_step();

        // Phase 4.1: Apply pending ECS-driven surface operations.
        let surface_ops = self.ecs_runtime.take_pending_surface_ops();
        self.surface_host.apply_pending_surface_ops(
            surface_ops,
            &mut self.size,
            &mut self.config,
            &self.surface,
            &self.device,
            &mut self.depth_texture,
            &mut self.depth_view,
            &mut self.viewport,
            &mut self.uv_ui.viewer,
            &mut self.ecs_runtime,
        );

        // Phase 5.1: Consume ECS-owned UI lifecycle operations.
        // Rendering code remains in host methods for now; this validates ECS stage orchestration.
        self.ecs_runtime.ui_frame_ops = self.ecs_runtime.take_pending_ui_frame_ops();

        // Phase 4.2: Apply pending prepare-stage GPU ops from ECS.
        let prepare_ops = self.ecs_runtime.take_pending_prepare_ops();
        if prepare_ops.update_main_camera_uniform {
            self.viewport.update_camera(&self.queue);
        }

        if let Some(ref rx) = self.asset_loader.gltf_rx {
            if let Ok(res) = rx.try_recv() {
            self.asset_loader.gltf_rx = None;
            self.asset_loader.gltf_loading_status = None;
            let path = self.asset_loader.loading_path.take().unwrap_or_default();
                match res {
                    Ok((doc, filename, reprojected_strokes)) => {
                        self.viewport.set_document(doc);
                        self.viewport.update_node_transforms(&self.queue);
                        self.focus_camera_on_model();
                        self.emit_ui_action(ecs::events::UiActionEvent::FinishGltfLoadSuccess {
                            filename,
                        });
                        self.process_ecs_step();
                        
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
                        self.emit_ui_action(ecs::events::UiActionEvent::FinishGltfLoadError {
                            path,
                            message: e,
                        });
                        self.process_ecs_step();
                    }
                }
            }
        }

        // Spawn/destroy the UV viewer window based on show_uv_viewer flag
        if self.app_state.ui().show_uv_viewer && self.uv_ui.viewer.is_none() {
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
            
            self.uv_ui.viewer = Some(UvViewerWindow {
                window,
                surface,
                config,
                egui_ctx,
                egui_state,
                egui_renderer,
                composite_tex_ids,
                layer_tex_ids,
            });
            self.ecs_runtime.set_uv_surface_size(size.width, size.height);
            self.ecs_runtime.set_uv_ui_window_active(true);
            self.ecs_runtime
                .update_uv_ui_resource(Some(self.uv_ui.viewer.as_ref().unwrap().egui_ctx.clone()), true, true);
            log::info!("Opened floatable UV Viewer window.");
        } else if !self.app_state.ui().show_uv_viewer && self.uv_ui.viewer.is_some() {
            self.uv_ui.viewer = None;
            self.ecs_runtime.clear_uv_surface();
            self.ecs_runtime.set_uv_ui_window_active(false);
            self.ecs_runtime.update_uv_ui_resource(None, false, false);
            log::info!("Closed floatable UV Viewer window.");
        }

        // Phase 5.1: Execute begin-frame lifecycle stage outside render paths.
        if self.ecs_runtime.ui_frame_ops.begin_main_egui_frame {
            let egui_input = self.main_ui.egui_state.take_egui_input(&*self.window);
            self.main_ui.egui_ctx.begin_pass(egui_input);
            self.main_ui.frame_begun = true;
        }
        if self.ecs_runtime.ui_frame_ops.begin_uv_egui_frame {
            if let Some(ref mut viewer) = self.uv_ui.viewer {
                let egui_input = viewer.egui_state.take_egui_input(&*viewer.window);
                viewer.egui_ctx.begin_pass(egui_input);
                self.uv_ui.frame_begun = true;
            }
        }

        // Phase 4.2: Execute render operations generated by ECS systems.
        self.ecs_runtime.sync_domain_state_from(&self.app_state);
        self.execute_pending_render_ops()
    }pub fn resize_uv_viewer(&mut self, width: u32, height: u32) {
        self.surface_host.resize_uv_surface(
            width,
            height,
            &mut self.uv_ui.viewer,
            &self.device,
            &mut self.ecs_runtime,
        );
    }

    pub fn queue_main_window_resize(&mut self, width: u32, height: u32) {
        self.surface_host
            .queue_main_window_resize(&mut self.ecs_runtime, width, height);
    }

    pub fn queue_uv_window_resize(&mut self, width: u32, height: u32) {
        self.surface_host
            .queue_uv_window_resize(&mut self.ecs_runtime, width, height);
    }

    pub fn queue_main_redraw(&mut self) {
        self.render_scheduler.queue_main_redraw(&mut self.ecs_runtime);
    }

    pub fn queue_uv_redraw(&mut self) {
        self.render_scheduler.queue_uv_redraw(&mut self.ecs_runtime);
    }

    pub fn execute_pending_render_ops(&mut self) -> Result<(), SurfaceError> {
        let render_ops = self.ecs_runtime.take_pending_render_ops();
        let should_render_main = self
            .render_scheduler
            .should_render_main_surface(&render_ops);

        if should_render_main {
            if let Err(err) = self.render() {
                self.render_host.handle_render_error(
                    &mut self.ecs_runtime,
                    ecs::events::RenderSurfaceKind::Main,
                    err,
                )?;
            }
        }
        if self.render_scheduler.should_render_uv_surface(&render_ops) {
            if let Err(err) = self.render_uv_viewer() {
                self.render_host.handle_render_error(
                    &mut self.ecs_runtime,
                    ecs::events::RenderSurfaceKind::Uv,
                    err,
                )?;
            }
        }
        Ok(())
    }

    pub fn set_uv_viewer_visible(&mut self, visible: bool) {
        self.emit_ui_action(ecs::events::UiActionEvent::SetUvViewerVisible(visible));
        self.process_ecs_step();
    }

    fn apply_pending_domain_host_ops(&mut self) {
        let pending = self.ecs_runtime.take_pending_domain_host_ops();
        for command in pending.input_state_commands {
            ecs::domain::apply_input_state_event(self, &command);
        }
        for command in pending.viewport_commands {
            ecs::domain::apply_viewport_event(self, &command);
        }
        for command in pending.tool_commands {
            let mut tool_system = {
                if let Some(mut runtime_tools) = self
                    .ecs_runtime
                    .world_mut()
                    .get_resource_mut::<crate::app::ecs::ToolRuntimeResource>()
                {
                    std::mem::take(&mut runtime_tools.0)
                } else {
                    crate::app::tools::ToolSystem::default()
                }
            };

            ecs::domain::apply_tool_event(self, &command, &mut tool_system);

            if let Some(mut runtime_tools) = self
                .ecs_runtime
                .world_mut()
                .get_resource_mut::<crate::app::ecs::ToolRuntimeResource>()
            {
                runtime_tools.0 = tool_system;
            }
        }
        for command in pending.document_commands {
            ecs::domain::apply_document_event(self, &command);
        }
        for action in pending.ui_actions {
            ecs::domain::apply_ui_event(self, &action);
        }
    }

    pub fn push_undo_state(&mut self) {
        // Clear redo stack when a new action is performed
        self.app_state.history_mut().redo_stack.clear();
        
        let undo_state = app_state::UndoState {
            layers: self.painter.layers.clone(),
            active_layer_idx: self.painter.active_layer_idx,
        };
        self.app_state.history_mut().undo_stack.push(undo_state);
        
        if self.app_state.history().undo_stack.len() > 50 {
            self.app_state.history_mut().undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev_state) = self.app_state.history_mut().undo_stack.pop() {
            let current_state = app_state::UndoState {
                layers: self.painter.layers.clone(),
                active_layer_idx: self.painter.active_layer_idx,
            };
            self.app_state.history_mut().redo_stack.push(current_state);
            
            self.painter.layers = prev_state.layers;
            self.painter.active_layer_idx = prev_state.active_layer_idx;
            
            self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
            log::info!("Performed Undo. Undo stack size: {}, Redo stack size: {}", self.app_state.history().undo_stack.len(), self.app_state.history().redo_stack.len());
        } else {
            log::info!("Nothing to undo");
        }
    }

    pub fn redo(&mut self) {
        if let Some(next_state) = self.app_state.history_mut().redo_stack.pop() {
            let current_state = app_state::UndoState {
                layers: self.painter.layers.clone(),
                active_layer_idx: self.painter.active_layer_idx,
            };
            self.app_state.history_mut().undo_stack.push(current_state);
            
            self.painter.layers = next_state.layers;
            self.painter.active_layer_idx = next_state.active_layer_idx;
            
            self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
            log::info!("Performed Redo. Undo stack size: {}, Redo stack size: {}", self.app_state.history().undo_stack.len(), self.app_state.history().redo_stack.len());
        } else {
            log::info!("Nothing to redo");
        }
    }
}

/// Re-renders a fill layer's GPU texture. Read-only operation with rendering side effects.
pub fn rerender_fill_layer(state: &State, idx: usize) {
    if idx >= state.painter.layers.len() || !state.painter.layers[idx].is_fill {
        return;
    }

    let layer = &state.painter.layers[idx];
    let base = [
        layer.fill_color[0] as f32 / 255.0,
        layer.fill_color[1] as f32 / 255.0,
        layer.fill_color[2] as f32 / 255.0,
        layer.fill_color[3] as f32 / 255.0,
    ];
    let noise = [
        layer.fill_noise_color[0] as f32 / 255.0,
        layer.fill_noise_color[1] as f32 / 255.0,
        layer.fill_noise_color[2] as f32 / 255.0,
        layer.fill_noise_color[3] as f32 / 255.0,
    ];
    state.painter.render_fill_layer(
        &state.device,
        &state.queue,
        idx,
        base,
        noise,
        layer.fill_noise_scale,
        layer.fill_projection_mode,
        &state.viewport.document,
    );
    state.painter.compose_layers(&state.device, &state.queue);
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

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn test_domain_flush_applies_ui_action_event() {
        #[cfg(target_os = "windows")]
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let mut builder = winit::event_loop::EventLoop::builder();
        #[cfg(target_os = "windows")]
        builder.with_any_thread(true);

        let event_loop = match builder.build() {
            Ok(loop_handle) => loop_handle,
            Err(_) => {
                eprintln!("Skipping test_domain_flush_applies_ui_action_event: no event loop");
                return;
            }
        };

        #[allow(deprecated)]
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_visible(false)
                .with_inner_size(winit::dpi::LogicalSize::new(320.0, 240.0)),
        ) {
            Ok(window) => Arc::new(window),
            Err(_) => {
                eprintln!("Skipping test_domain_flush_applies_ui_action_event: no window backend");
                return;
            }
        };

        let mut state = match pollster::block_on(State::new(window)) {
            Ok(state) => state,
            Err(_) => {
                eprintln!("Skipping test_domain_flush_applies_ui_action_event: state init failed");
                return;
            }
        };

        state
            .ecs_runtime
            .send_event(ecs::events::AppEvent::Ui(ecs::events::UiActionEvent::SetBrushSize(
                111.0,
            )));
        state.process_ecs_step();

        assert_eq!(state.app_state.canvas().brush_size, 111.0);
    }
}

