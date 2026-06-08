mod actions;
mod app_state;
pub mod ecs;
pub(crate) mod input;
mod input_editor;
mod render;
mod surface;
mod tools;
mod types;
mod ui;
mod ui_panels;
mod user_preferences;

pub use surface::UvViewerWindow;
pub(crate) use surface::{
    RenderHostCoordinator, RenderSchedulingCoordinator, SurfaceHostCoordinator,
};
pub use types::{LoadError, SurfaceError, Tool};

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;
use user_preferences::UserPreferences;
use winit::window::Window;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;

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

type GltfLoadResult = Result<
    (
        crate::mesh::Document,
        String,
        Vec<Vec<crate::painter::PaintStroke>>,
    ),
    String,
>;

#[derive(Default)]
pub(crate) struct AssetLoadCoordinator {
    gltf_rx: Option<std::sync::mpsc::Receiver<GltfLoadResult>>,
    gltf_loading_status: Option<Arc<std::sync::Mutex<String>>>,
    loading_path: Option<std::path::PathBuf>,
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
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,

    // Render pipeline & logic state
    viewport: crate::viewport::Viewport,

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
    pub(crate) fn painter(&self) -> &crate::painter::Painter {
        &self.ecs_runtime.world().get_resource::<ecs::PainterResource>().expect("PainterResource").0
    }

    pub(crate) fn painter_mut(&mut self) -> &mut crate::painter::Painter {
        &mut self.ecs_runtime.world_mut().get_resource_mut::<ecs::PainterResource>().expect("PainterResource").into_inner().0
    }

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

    /// Emit an ECS event into the runtime's event queue.
    ///
    /// Runtime input and command routing is ECS-native.
    pub fn emit_event(&mut self, event: ecs::events::AppEvent) {
        self.ecs_runtime.send_event(event);
    }

    fn emit_ui_action(&mut self, action: ecs::events::UiActionEvent) {
        self.emit_event(ecs::events::AppEvent::Ui(action));
    }

    pub fn update(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<(), SurfaceError> {
        // Set the active State pointer in the ECS world before ticking.
        // This allows ECS systems to directly access host methods (drawing, rendering, WGPU context) unsafely but correctly.
        let ptr = self as *mut State;
        self.ecs_runtime
            .world_mut()
            .insert_resource(ecs::HostStatePtr(ptr));

        // 1. Synchronize host app state to ECS domain resource before tick.
        self.ecs_runtime.sync_domain_state_from(&self.app_state);

        // 2. Tick the Bevy ECS schedule.
        // Resizing, GPU preparation, Egui frame starting, and WGPU rendering are run natively inside systems.
        self.ecs_runtime.tick();

        // 3. Consume pending host-side domain mutations produced by ECS systems.
        self.apply_pending_domain_host_ops();

        // 4. Tick asset loading and child window lifecycles on the host.
        self.tick_asset_loader();
        self.tick_window_lifecycle(event_loop);

        // 5. Propagate any rendering errors encountered during ECS system runs.
        if let Some(err) = self.ecs_runtime.take_render_error() {
            return Err(err);
        }

        Ok(())
    }

    fn tick_asset_loader(&mut self) {
        if let Some(ref rx) = self.asset_loader.gltf_rx {
            if let Ok(res) = rx.try_recv() {
                self.asset_loader.gltf_rx = None;
                self.asset_loader.gltf_loading_status = None;
                let path = self.asset_loader.loading_path.take().unwrap_or_default();
                match res {
                    Ok((doc, filename, reprojected_strokes)) => {
                        self.ecs_runtime.register_document(doc);
                        self.viewport.update_node_transforms(self.ecs_runtime.world_mut(), &self.queue);
                        self.focus_camera_on_model();
                        self.emit_ui_action(ecs::events::UiActionEvent::FinishGltfLoadSuccess {
                            filename,
                        });
                        self.process_ecs_step();

                        // Assign background reprojected strokes back to layers
                        let mut layers_query = self.ecs_runtime.world_mut().query::<(
                            &ecs::LayerIndex,
                            &mut ecs::LayerStrokes,
                            Option<&ecs::FillLayerProperties>,
                        )>();
                        let mut layers: Vec<_> = layers_query.iter_mut(self.ecs_runtime.world_mut()).collect();
                        layers.sort_by_key(|(idx, _, _)| idx.0);

                        for (layer_idx, strokes) in reprojected_strokes.into_iter().enumerate() {
                            if layer_idx < layers.len() {
                                if layers[layer_idx].2.is_none() { // not a fill layer
                                    layers[layer_idx].1.0 = strokes;
                                }
                            }
                        }

                        self.ecs_runtime.redraw_all_layers_ecs(&self.device, &self.queue);
                        log::info!(
                            "Successfully loaded glTF model — strokes reprojected in background"
                        );
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
    }

    fn tick_window_lifecycle(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Spawn/destroy the UV viewer window based on show_uv_viewer flag
        if self.app_state.ui().show_uv_viewer && self.uv_ui.viewer.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("UV Viewer")
                            .with_inner_size(winit::dpi::LogicalSize::new(800, 600)),
                    )
                    .unwrap(),
            );

            // Create surface for the child window
            let surface = self.instance.create_surface(window.clone()).unwrap();

            let caps = surface.get_capabilities(&self.adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);

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
            for view in &self.painter().composite_views {
                let id = egui_renderer.register_native_texture(
                    &self.device,
                    view,
                    wgpu::FilterMode::Linear,
                );
                composite_tex_ids.push(id);
            }

            let mut layer_tex_ids = Vec::new();
            for view in &self.painter().layer_views {
                let id = egui_renderer.register_native_texture(
                    &self.device,
                    view,
                    wgpu::FilterMode::Linear,
                );
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
            self.ecs_runtime
                .set_uv_surface_size(size.width, size.height);
            self.ecs_runtime.set_uv_ui_window_active(true);
            self.ecs_runtime.update_uv_ui_resource(
                Some(self.uv_ui.viewer.as_ref().unwrap().egui_ctx.clone()),
                true,
                true,
            );
            log::info!("Opened floatable UV Viewer window.");
        } else if !self.app_state.ui().show_uv_viewer && self.uv_ui.viewer.is_some() {
            self.uv_ui.viewer = None;
            self.ecs_runtime.clear_uv_surface();
            self.ecs_runtime.set_uv_ui_window_active(false);
            self.ecs_runtime.update_uv_ui_resource(None, false, false);
            log::info!("Closed floatable UV Viewer window.");
        }
    }


    pub(crate) fn commit_current_stroke(&mut self) {
        if let Some(stroke) = self.interaction.stroke_in_progress.take() {
            let has_fill = {
                let world = self.ecs_runtime.world_mut();
                let mut active_query = world.query_filtered::<
                    Option<&ecs::FillLayerProperties>,
                    With<ecs::ActiveLayer>
                >();
                active_query.get_single(world).ok().flatten().is_some()
            };

            if !has_fill {
                self.push_undo_state();
                let world = self.ecs_runtime.world_mut();
                let mut active_query = world.query_filtered::<
                    &mut ecs::LayerStrokes,
                    With<ecs::ActiveLayer>
                >();
                if let Ok(mut strokes) = active_query.get_single_mut(world) {
                    strokes.0.push(stroke);
                }
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

    pub(crate) fn sync_app_state_snapshot(&mut self) {
        let active_layer_idx = {
            let world = self.ecs_runtime.world_mut();
            let mut active_query = world.query_filtered::<&ecs::LayerIndex, With<ecs::ActiveLayer>>();
            active_query.iter(world).next().map(|idx| idx.0).unwrap_or(0)
        };
        let layer_count = {
            let world = self.ecs_runtime.world_mut();
            let mut name_query = world.query_filtered::<Entity, With<ecs::LayerName>>();
            name_query.iter(world).count()
        };
        let num_udim_tiles = self.ecs_runtime.world().get_resource::<ecs::DocumentResource>().map(|d| d.document.num_udim_tiles).unwrap_or(1);

        self.app_state.document_mut().active_layer_idx = active_layer_idx;
        self.app_state.document_mut().layer_count = layer_count;
        self.app_state.document_mut().num_udim_tiles = num_udim_tiles as u32;

        self.app_state.history_mut().undo_len = self.app_state.history().undo_stack.len();
        self.app_state.history_mut().redo_len = self.app_state.history().redo_stack.len();

        self.app_state.input_mut().pan_modifier = self.app_state.input().alt;

        // Sync camera data
        if let Some(camera) = self.ecs_runtime.world().get_resource::<ecs::CameraResource>() {
            let camera_mut = self.app_state.camera_mut();
            camera_mut.eye = camera.get_eye();
            camera_mut.target = camera.target;
            camera_mut.yaw = camera.yaw;
            camera_mut.pitch = camera.pitch;
            camera_mut.distance = camera.distance;
            camera_mut.fov = camera.fovy;
            camera_mut.aspect = camera.aspect;
        }

        // Sync layer composition data
        let mut layers: Vec<_> = {
            let world = self.ecs_runtime.world_mut();
            let mut query = world.query::<(&ecs::LayerIndex, &ecs::LayerVisibility, &ecs::LayerOpacity)>();
            query
                .iter(world)
                .map(|(idx, vis, opacity)| (idx.0, vis.0, opacity.0))
                .collect()
        };
        layers.sort_by_key(|(idx, _, _)| *idx);
        let composition_mut = self.app_state.layer_composition_mut();
        composition_mut.visibilities = layers.iter().map(|(_, vis, _)| *vis).collect();
        composition_mut.opacities = layers.iter().map(|(_, _, opacity)| *opacity).collect();
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
                if let Some(mut runtime_tools) =
                    self.ecs_runtime
                        .world_mut()
                        .get_resource_mut::<crate::app::ecs::ToolRuntimeResource>()
                {
                    std::mem::take(&mut runtime_tools.0)
                } else {
                    crate::app::tools::ToolSystem::default()
                }
            };

            ecs::domain::apply_tool_event(self, &command, &mut tool_system);

            if let Some(mut runtime_tools) =
                self.ecs_runtime
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

        let active_layer_idx = self.app_state.document().active_layer_idx;
        let undo_state = app_state::UndoState {
            layers: self.ecs_runtime.get_layers_snapshot(),
            active_layer_idx,
        };
        self.app_state.history_mut().undo_stack.push(undo_state);

        if self.app_state.history().undo_stack.len() > 50 {
            self.app_state.history_mut().undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev_state) = self.app_state.history_mut().undo_stack.pop() {
            let active_layer_idx = self.app_state.document().active_layer_idx;
            let current_state = app_state::UndoState {
                layers: self.ecs_runtime.get_layers_snapshot(),
                active_layer_idx,
            };
            self.app_state.history_mut().redo_stack.push(current_state);

            self.app_state.document_mut().active_layer_idx = prev_state.active_layer_idx;
            self.app_state.document_mut().layer_count = prev_state.layers.len();

            self.ecs_runtime.restore_layers_snapshot(&prev_state.layers, prev_state.active_layer_idx, &self.device);

            self.ecs_runtime
                .redraw_all_layers_ecs(&self.device, &self.queue);
            log::info!(
                "Performed Undo. Undo stack size: {}, Redo stack size: {}",
                self.app_state.history().undo_stack.len(),
                self.app_state.history().redo_stack.len()
            );
        } else {
            log::info!("Nothing to undo");
        }
    }

    pub fn redo(&mut self) {
        if let Some(next_state) = self.app_state.history_mut().redo_stack.pop() {
            let active_layer_idx = self.app_state.document().active_layer_idx;
            let current_state = app_state::UndoState {
                layers: self.ecs_runtime.get_layers_snapshot(),
                active_layer_idx,
            };
            self.app_state.history_mut().undo_stack.push(current_state);

            self.app_state.document_mut().active_layer_idx = next_state.active_layer_idx;
            self.app_state.document_mut().layer_count = next_state.layers.len();

            self.ecs_runtime.restore_layers_snapshot(&next_state.layers, next_state.active_layer_idx, &self.device);

            self.ecs_runtime
                .redraw_all_layers_ecs(&self.device, &self.queue);
            log::info!(
                "Performed Redo. Undo stack size: {}, Redo stack size: {}",
                self.app_state.history().undo_stack.len(),
                self.app_state.history().redo_stack.len()
            );
        } else {
            log::info!("Nothing to redo");
        }
    }
}

/// Re-renders a fill layer's GPU texture. Read-only operation with rendering side effects.
pub fn rerender_fill_layer(state: &mut State, idx: usize) {
    let (views_cloned, base, noise, fill_noise_scale, fill_projection_mode) = {
        let mut query = state.ecs_runtime.world_mut().query::<(
            &ecs::LayerIndex,
            &ecs::LayerTexture,
            &ecs::FillLayerProperties,
        )>();

        let Some((_, texture_comp, fill)) = query
            .iter(state.ecs_runtime.world_mut())
            .find(|(index, _, _)| index.0 == idx) else {
                return;
            };

        let base = [
            fill.color[0] as f32 / 255.0,
            fill.color[1] as f32 / 255.0,
            fill.color[2] as f32 / 255.0,
            fill.color[3] as f32 / 255.0,
        ];
        let noise = [
            fill.noise_color[0] as f32 / 255.0,
            fill.noise_color[1] as f32 / 255.0,
            fill.noise_color[2] as f32 / 255.0,
            fill.noise_color[3] as f32 / 255.0,
        ];

        (texture_comp.views.clone(), base, noise, fill.noise_scale, fill.projection_mode)
    };

    let mut nodes = Vec::new();
    {
        let world = state.ecs_runtime.world_mut();
        let mut mesh_query = world.query::<(&ecs::MeshHandle, &ecs::NodeGpuResources)>();
        for (mesh, gpu_res) in mesh_query.iter(world) {
            nodes.push((mesh.0.clone(), gpu_res.bind_group.clone()));
        }
    }

    let view_refs: Vec<&wgpu::TextureView> = views_cloned.iter().map(|v| &**v).collect();
    let node_refs: Vec<(&crate::mesh::Mesh, &wgpu::BindGroup)> = nodes
        .iter()
        .map(|(m, bg)| (&**m, &**bg))
        .collect();

    state.painter().render_fill_layer_to_views(
        &state.device,
        &state.queue,
        &view_refs,
        base,
        noise,
        fill_noise_scale,
        fill_projection_mode,
        &node_refs,
    );

    // Call compose_layers_ecs to update the composition
    let mut compose_query = state.ecs_runtime.world_mut().query::<(&ecs::LayerIndex, &ecs::LayerVisibility, &ecs::LayerOpacity, &ecs::LayerBlendMode)>();
    let mut compose_layers: Vec<_> = compose_query.iter(state.ecs_runtime.world_mut()).collect();
    compose_layers.sort_by_key(|(idx, _, _, _)| idx.0);

    let active_layers_data: Vec<(f32, crate::painter::BlendMode, bool)> = compose_layers
        .iter()
        .map(|(_, vis, opacity, blend)| (opacity.0, blend.0, vis.0))
        .collect();

    state.painter().compose_layers_ecs(&state.device, &state.queue, &active_layers_data);
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

        state.ecs_runtime.send_event(ecs::events::AppEvent::Ui(
            ecs::events::UiActionEvent::SetBrushSize(111.0),
        ));
        state.process_ecs_step();

        assert_eq!(state.app_state.canvas().brush_size, 111.0);
    }
}
