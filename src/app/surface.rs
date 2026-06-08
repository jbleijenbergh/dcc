use super::app_state;
use super::ecs;
use super::ui;
use super::user_preferences::UserPreferences;
use super::{HostEcsRuntime, MainUiRuntime, State, SurfaceError};
use std::sync::Arc;
use winit::window::Window;

pub(crate) struct RenderHostCoordinator {
    supported_present_modes: Vec<wgpu::PresentMode>,
}

impl RenderHostCoordinator {
    pub(crate) fn new(supported_present_modes: Vec<wgpu::PresentMode>) -> Self {
        Self {
            supported_present_modes,
        }
    }

    pub(crate) fn supports_present_mode(&self, mode: wgpu::PresentMode) -> bool {
        self.supported_present_modes.contains(&mode)
    }

    pub(crate) fn handle_render_error(
        &self,
        ecs_runtime: &mut ecs::EcsRuntime,
        surface: ecs::events::RenderSurfaceKind,
        err: SurfaceError,
    ) -> Result<(), SurfaceError> {
        match err {
            SurfaceError::Lost => {
                ecs_runtime.send_render_failure_event(ecs::events::RenderFailureEvent {
                    surface,
                    kind: ecs::events::RenderFailureKind::Lost,
                })
            }
            SurfaceError::Outdated => {
                ecs_runtime.send_render_failure_event(ecs::events::RenderFailureEvent {
                    surface,
                    kind: ecs::events::RenderFailureKind::Outdated,
                })
            }
            SurfaceError::Timeout => {}
            SurfaceError::Other(e) => return Err(SurfaceError::Other(e)),
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct SurfaceHostCoordinator;

impl SurfaceHostCoordinator {
    pub(crate) fn apply_pending_surface_ops(
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

    pub(crate) fn resize_main_surface(
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

    pub(crate) fn resize_uv_surface(
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

    pub(crate) fn queue_main_window_resize(
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

    pub(crate) fn queue_uv_window_resize(
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
    pub(crate) fn queue_main_redraw(&self, ecs_runtime: &mut HostEcsRuntime) {
        ecs_runtime.send_redraw_event(ecs::events::RedrawEvent::MainSurface);
    }

    pub(crate) fn queue_uv_redraw(&self, ecs_runtime: &mut HostEcsRuntime) {
        ecs_runtime.send_redraw_event(ecs::events::RedrawEvent::UvSurface);
    }

    pub(crate) fn should_render_main_surface(
        &self,
        render_ops: &ecs::PendingRenderOpsResource,
    ) -> bool {
        render_ops.render_main_surface
            || render_ops.render_3d_viewport_pass
            || render_ops.render_paint_composite_pass
    }

    pub(crate) fn should_render_uv_surface(
        &self,
        render_ops: &ecs::PendingRenderOpsResource,
    ) -> bool {
        render_ops.render_uv_surface
    }
}

impl State {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        log::info!(
            "Creating State with window size: {}x{}",
            size.width,
            size.height
        );

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

        let (depth_texture, depth_view) =
            crate::viewport::create_depth_texture(&device, &config, "depth_texture");

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

        // Store active window and combined render context directly in ECS resources
        ecs_runtime.world_mut().insert_resource(ecs::WindowResource(window.clone()));
        ecs_runtime.world_mut().insert_resource(ecs::MainRenderContextResource {
            surface,
            config,
            depth_texture,
            depth_view,
        });

        let state = Self {
            window,
            device,
            queue,
            size,
            viewport,
            painter,
            main_ui: MainUiRuntime::new(egui_ctx, egui_state, egui_renderer),
            instance,
            adapter,
            uv_ui: super::UvUiRuntime::default(),
            asset_loader: super::AssetLoadCoordinator::default(),
            render_host: RenderHostCoordinator::new(surface_caps.present_modes),
            surface_host: SurfaceHostCoordinator::default(),
            render_scheduler: RenderSchedulingCoordinator::default(),
            import_settings: crate::mesh::ImportSettings {
                seams_option: crate::mesh::SeamsOption::GenerateMissing,
                margin_size: crate::mesh::MarginSize::Medium,
                island_orientation: crate::mesh::IslandOrientation::AlignWith3DMesh,
            },
            interaction: super::InteractionState::default(),

            preferences,
            preferences_path,
            ui_state: ui::TransientUiState::default(),
            app_state,
            ecs_runtime: super::HostEcsRuntime::new(ecs_runtime),
        };

        if !state.preferences_path.exists() {
            if let Err(e) = state.preferences.save_to(&state.preferences_path) {
                log::warn!("Failed to create default settings file: {}", e);
            }
        }

        Ok(state)
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        let mut main_ctx = self.ecs_runtime.world_mut().get_resource_mut::<ecs::MainRenderContextResource>().unwrap();
        let main_ctx = &mut *main_ctx;
        self.surface_host.resize_main_surface(
            new_size,
            &mut self.size,
            &mut main_ctx.config,
            &main_ctx.surface,
            &self.device,
            &mut main_ctx.depth_texture,
            &mut main_ctx.depth_view,
            &mut self.viewport,
        );
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.ecs_runtime.world().get_resource::<ecs::MainRenderContextResource>().unwrap().config.format
    }

    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.ecs_runtime.world().get_resource::<ecs::MainRenderContextResource>().unwrap().config.present_mode
    }

    pub fn configure_surface(&mut self, present_mode: wgpu::PresentMode) {
        let mut main_ctx = self.ecs_runtime.world_mut().get_resource_mut::<ecs::MainRenderContextResource>().unwrap();
        main_ctx.config.present_mode = present_mode;
        main_ctx.surface.configure(&self.device, &main_ctx.config);
    }

    pub fn resize_uv_viewer(&mut self, width: u32, height: u32) {
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
        self.render_scheduler
            .queue_main_redraw(&mut self.ecs_runtime);
    }

    pub fn queue_uv_redraw(&mut self) {
        self.render_scheduler.queue_uv_redraw(&mut self.ecs_runtime);
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
