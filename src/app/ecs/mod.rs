//! ECS runtime layer for the DCC application.
//!
//! This module provides a Bevy ECS + App infrastructure that integrates
//! the winit event loop and wgpu rendering pipeline with ECS-driven
//! state management and scheduling.

use bevy_ecs::prelude::*;

mod plugins;
pub mod domain;

/// Phase 3: System sets for deterministic frame flow.
/// Each set represents a logical phase in the frame.
/// 
/// Frame flow:
/// 1. WinitEventIngest - Poll and ingest winit events
/// 2. InputResolve - Normalize input state, update modifiers
/// 3. DomainUpdate - Process domain state changes from events
/// 4. ExtractRenderData - Extract camera, layers, compose render targets
/// 5. PrepareGpu - Upload texture data, update buffers
/// 6. RenderMainSurface - Render 3D viewport + UI overlay
/// 7. RenderAuxSurfaces - Render auxiliary surfaces (UV viewer, etc.)
/// 8. EndFrame - Present surface, cleanup transients
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FramePhase {
    /// Poll and ingest winit events into ECS event queue.
    WinitEventIngest,
    /// Normalize and resolve input state from events.
    InputResolve,
    /// Process domain state changes from command events.
    DomainUpdate,
    /// Extract render data from mutable resources (read phase).
    ExtractRenderData,
    /// Prepare GPU resources: upload textures, update buffers.
    PrepareGpu,
    /// Render 3D viewport and UI overlay to main surface.
    RenderMainSurface,
    /// Render auxiliary surfaces (UV viewer, preview, etc.).
    RenderAuxSurfaces,
    /// Present surface and cleanup transients.
    EndFrame,
}

/// ECS event types mirroring the message bus.
/// These events are emitted by input systems and consumed by action systems.
pub mod events {
    use bevy_ecs::event::Event;
    use crate::app::input::{ModifiersSnapshot, PointerData};
    use crate::painter::BlendMode;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ToolKind {
        Brush,
        Eraser,
    }

    #[derive(Clone, Debug, Event)]
    pub enum UiActionEvent {
        SelectTool(ToolKind),
        AdjustBrushSize(f32),
        SetBrushSize(f32),
        SetBrushHardness(f32),
        SetBrushOpacity(f32),
        SetBrushColor([u8; 4]),
        SetUvViewerVisible(bool),
        SetUvViewerSource(usize),
        SetUvViewerSize(f32),
        SetUvWireframe(bool),
        SwitchMesh(String),
        SetCurrentMesh(String),
        SetActiveScene(usize),
        SetImportSeams(crate::mesh::SeamsOption),
        SetImportMargin(crate::mesh::MarginSize),
        SetImportOrientation(crate::mesh::IslandOrientation),
        RecomputeUvsAndReproject,
        SetPressureCurve { min_start: f32, max_at: f32 },
        StartGltfLoad,
        FinishGltfLoadSuccess { filename: String },
        FinishGltfLoadError { path: std::path::PathBuf, message: String },
        DismissLoadError,
        SelectLayer(usize),
        AddPaintLayer(String),
        AddUvGridLayer,
        AddUvCheckerLayer,
        AddFillLayer,
        DeleteLayer(usize),
        SetLayerVisible { idx: usize, visible: bool },
        SetLayerBlendMode { idx: usize, mode: BlendMode },
        SetLayerOpacity { idx: usize, opacity: f32, begin_undo: bool },
        SetFillBaseColor { idx: usize, color: [u8; 4], begin_undo: bool },
        SetFillNoiseColor { idx: usize, color: [u8; 4], begin_undo: bool },
        SetFillNoiseScale { idx: usize, scale: f32, begin_undo: bool },
        SetFillProjectionMode { idx: usize, mode: u32 },
        ClearCanvas,
        Undo,
        Redo,
    }

    #[derive(Clone, Debug, Event)]
    pub enum DocumentCommandEvent {
        CommitCurrentStroke,
        ClearAllLayers,
    }

    #[derive(Clone, Copy, Debug, Event)]
    pub enum ViewportCommandEvent {
        Orbit { dx: f32, dy: f32 },
        Pan { dx: f32, dy: f32 },
        Zoom { scroll: f32 },
    }

    #[derive(Clone, Copy, Debug, Event)]
    pub enum ToolCommandEvent {
        PointerDown(PointerData),
        PointerMove(PointerData),
        PointerUp(PointerData),
        PointerCancel(PointerData),
    }

    #[derive(Clone, Copy, Debug, Event)]
    pub enum InputStateCommandEvent {
        UpdateModifier { key: winit::keyboard::KeyCode, is_pressed: bool },
        UpdateModifiersSnapshot(ModifiersSnapshot),
        UpdateMousePosition(PointerData),
        SetPaintButtonDown(bool),
        SetPanButtonDown(bool),
        SetOrbitModifier(bool),
        SetAltModifier(bool),
        ResetPenPressure,
    }

    /// Top-level ECS event enum mirroring Message.
    #[derive(Clone, Debug, Event)]
    pub enum AppEvent {
        Ui(UiActionEvent),
        Document(DocumentCommandEvent),
        Viewport(ViewportCommandEvent),
        Tool(ToolCommandEvent),
        InputState(InputStateCommandEvent),
    }

    /// Window/surface lifecycle events for Phase 4 integration.
    #[derive(Clone, Copy, Debug, Event)]
    pub enum WindowSurfaceEvent {
        MainWindowResized { width: u32, height: u32 },
        UvWindowResized { width: u32, height: u32 },
    }

    /// Render request events for ECS-controlled render pass orchestration.
    #[derive(Clone, Copy, Debug, Event)]
    pub enum RenderRequestEvent {
        MainSurface,
        UvSurface,
    }

    /// Redraw intent events from the host window loop.
    #[derive(Clone, Copy, Debug, Event)]
    pub enum RedrawEvent {
        MainSurface,
        UvSurface,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum RenderSurfaceKind {
        Main,
        Uv,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum RenderFailureKind {
        Lost,
        Outdated,
    }

    /// Render backend failure signal consumed by ECS recovery systems.
    #[derive(Clone, Copy, Debug, Event)]
    pub struct RenderFailureEvent {
        pub surface: RenderSurfaceKind,
        pub kind: RenderFailureKind,
    }
}

/// Resource wrapper for the application domain state snapshot.
/// 
/// Holds the read-only or shared portions of app state that need to be
/// accessible from multiple systems without direct mutability contention.
#[derive(Resource, Clone)]
pub struct DomainStateResource(pub crate::app::app_state::AppState);

/// Resource wrapper for user interaction state.
///
/// Tracks stroke-in-progress, last hit coordinates, and other transient
/// per-frame interaction data.
#[derive(Resource, Default)]
pub struct InteractionStateResource {
    pub stroke_in_progress: Option<crate::painter::PaintStroke>,
    pub last_hit_uv: Option<glam::Vec2>,
    pub last_hit_pos: Option<glam::Vec3>,
}

/// Resource wrapper for user preferences and settings.
#[derive(Resource, Clone)]
pub struct PreferencesResource(pub crate::app::user_preferences::UserPreferences);

/// Resource wrapper for transient UI state (errors, feedback, etc).
#[derive(Resource, Clone)]
pub struct UiStateResource(pub crate::app::ui::TransientUiState);

/// Resource wrapper for GPU context handles.
///
/// Stores the wgpu instance, adapter, device, and queue in a form
/// accessible to render systems.
#[derive(Resource)]
pub struct GpuContextResource {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: std::sync::Arc<wgpu::Device>,
    pub queue: wgpu::Queue,
}

/// Phase 4.1: Surface registry tracked by ECS.
#[derive(Resource, Clone, Copy, Debug)]
pub struct SurfaceRegistryResource {
    pub main_surface_size: (u32, u32),
    pub uv_surface_size: Option<(u32, u32)>,
}

impl Default for SurfaceRegistryResource {
    fn default() -> Self {
        Self {
            main_surface_size: (1, 1),
            uv_surface_size: None,
        }
    }
}

/// Phase 4.1: Pending resize operations emitted by ECS systems and applied by host state.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PendingSurfaceOpsResource {
    pub main_resize: Option<(u32, u32)>,
    pub uv_resize: Option<(u32, u32)>,
}

/// Phase 4.2: Pending render operations emitted by ECS render systems.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PendingRenderOpsResource {
    pub render_main_surface: bool,
    pub render_3d_viewport_pass: bool,
    pub render_paint_composite_pass: bool,
    pub render_uv_surface: bool,
}

/// Phase 4.2: Pending prepare-stage GPU operations emitted by ECS systems.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PendingPrepareOpsResource {
    pub update_main_camera_uniform: bool,
}

/// Pending host-side domain operations emitted by ECS mutation systems.
///
/// These operations represent non-ECS side effects that still require host adapters
/// (painter, viewport, undo/redo, import settings, etc).
#[derive(Resource, Clone, Debug, Default)]
pub struct PendingDomainHostOpsResource {
    pub ui_actions: Vec<events::UiActionEvent>,
    pub document_commands: Vec<events::DocumentCommandEvent>,
    pub viewport_commands: Vec<events::ViewportCommandEvent>,
    pub tool_commands: Vec<events::ToolCommandEvent>,
    pub input_state_commands: Vec<events::InputStateCommandEvent>,
}

/// Phase 5.1: Per-window UI registry owned by ECS.
#[derive(Resource, Clone, Copy, Debug)]
pub struct UiWindowRegistryResource {
    pub main_window_ui_active: bool,
    pub uv_window_ui_active: bool,
}

impl Default for UiWindowRegistryResource {
    fn default() -> Self {
        Self {
            main_window_ui_active: true,
            uv_window_ui_active: false,
        }
    }
}

/// Phase 5.1: Pending UI frame lifecycle operations emitted by ECS systems.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PendingUiFrameOpsResource {
    pub begin_main_egui_frame: bool,
    pub draw_main_egui_panels: bool,
    pub end_main_egui_frame_and_upload: bool,
    pub begin_uv_egui_frame: bool,
    pub draw_uv_egui_panels: bool,
    pub end_uv_egui_frame_and_upload: bool,
}

/// Phase 5.1: ECS resource for the main window's egui runtime handles.
#[derive(Resource, Clone)]
pub struct MainWindowUiResource {
    pub egui_ctx: egui::Context,
    pub has_winit_state: bool,
    pub has_renderer: bool,
}

/// Phase 5.1: ECS resource for the UV window's egui runtime handles.
#[derive(Resource, Clone, Default)]
pub struct UvWindowUiResource {
    pub egui_ctx: Option<egui::Context>,
    pub has_winit_state: bool,
    pub has_renderer: bool,
}

/// Runtime-owned tool handlers persisted across frames/events.
#[derive(Resource, Default)]
pub struct ToolRuntimeResource(pub crate::app::tools::ToolSystem);

/// Phase 3.2: Extracted render data for read-only render stages.
///
/// These resources are populated during ExtractRenderData phase and read-only
/// during RenderMainSurface/RenderAuxSurfaces phases. This ensures render systems
/// cannot mutate domain state, preventing frame-time issues and race conditions.

/// Extracted camera state for rendering.
/// Populated during ExtractRenderData from DomainStateResource.
#[derive(Resource, Clone, Copy, Default)]
pub struct ExtractedCameraData {
    pub eye: glam::Vec3,
    pub target: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov: f32,
    pub aspect: f32,
}

/// Extracted layer composition state for rendering.
/// Populated during ExtractRenderData from DomainStateResource.
#[derive(Resource, Clone)]
pub struct ExtractedLayerComposition {
    pub layer_count: usize,
    pub active_layer_idx: usize,
    /// Per-layer visibility state
    pub layer_visibility: Vec<bool>,
    /// Per-layer opacity values
    pub layer_opacities: Vec<f32>,
}

impl Default for ExtractedLayerComposition {
    fn default() -> Self {
        Self {
            layer_count: 0,
            active_layer_idx: 0,
            layer_visibility: Vec::new(),
            layer_opacities: Vec::new(),
        }
    }
}

/// Extracted document metadata for rendering.
/// Populated during ExtractRenderData from DomainStateResource.
#[derive(Resource, Clone, Default)]
pub struct ExtractedDocumentData {
    pub current_mesh: String,
    pub num_udim_tiles: usize,
    pub num_paint_layers: usize,
}

/// The central ECS runtime container.
///
/// Holds the World and Schedule objects that will be ticked each frame.
pub struct EcsRuntime {
    pub world: World,
    pub schedule: Schedule,
}

impl EcsRuntime {
    /// Create a new ECS runtime with an empty world and default schedule.
    pub fn new() -> Self {
        let mut world = World::new();
        
        // Register event types so they can be sent/received in systems
        world.init_resource::<Events<events::AppEvent>>();
        world.init_resource::<Events<events::WindowSurfaceEvent>>();
        world.init_resource::<Events<events::RedrawEvent>>();
        world.init_resource::<Events<events::RenderRequestEvent>>();
        world.init_resource::<Events<events::RenderFailureEvent>>();
        world.insert_resource(DomainStateResource(
            crate::app::app_state::AppState::new(),
        ));
        world.init_resource::<SurfaceRegistryResource>();
        world.init_resource::<PendingSurfaceOpsResource>();
        world.init_resource::<PendingPrepareOpsResource>();
        world.init_resource::<PendingDomainHostOpsResource>();
        world.init_resource::<PendingRenderOpsResource>();
        world.init_resource::<UiWindowRegistryResource>();
        world.init_resource::<PendingUiFrameOpsResource>();
        world.init_resource::<ToolRuntimeResource>();
        
        // Phase 3: Initialize ordered schedules for deterministic frame flow.
        // The main schedule will execute each phase in order.
        let mut schedule = Schedule::default();

        // Phase 6.1 plugin-style registration groups.
        plugins::core::register(&mut schedule);
        plugins::input::register(&mut schedule);
        plugins::document::register(&mut schedule);
        plugins::tool::register(&mut schedule);
        plugins::render::register(&mut schedule);
        plugins::ui::register(&mut schedule);
        plugins::asset_io::register(&mut schedule);
        
        // Initialize extracted render data resources for Phase 3.2
        // These will be populated during ExtractRenderData phase
        world.init_resource::<ExtractedCameraData>();
        world.init_resource::<ExtractedLayerComposition>();
        world.init_resource::<ExtractedDocumentData>();
        
        Self { world, schedule }
    }

    /// Register a domain state resource into the world.
    pub fn register_domain_state(&mut self, app_state: crate::app::app_state::AppState) {
        self.world.insert_resource(DomainStateResource(app_state));
    }

    /// Synchronize host-owned app state into ECS domain resource.
    ///
    /// This is the explicit contract while domain mutation is still host-driven.
    /// Call once per frame before ECS schedule execution.
    pub fn sync_domain_state_from(&mut self, app_state: &crate::app::app_state::AppState) {
        if let Some(mut domain) = self.world.get_resource_mut::<DomainStateResource>() {
            domain.0 = app_state.clone();
        } else {
            self.world.insert_resource(DomainStateResource(app_state.clone()));
        }
    }

    /// Register interaction state into the world.
    pub fn register_interaction_state(&mut self, interaction: InteractionStateResource) {
        self.world.insert_resource(interaction);
    }

    /// Register preferences into the world.
    pub fn register_preferences(&mut self, prefs: crate::app::user_preferences::UserPreferences) {
        self.world.insert_resource(PreferencesResource(prefs));
    }

    /// Register main-window egui context and availability flags.
    pub fn register_main_ui_resource(
        &mut self,
        egui_ctx: egui::Context,
        has_winit_state: bool,
        has_renderer: bool,
    ) {
        self.world.insert_resource(MainWindowUiResource {
            egui_ctx,
            has_winit_state,
            has_renderer,
        });
    }

    /// Update UV-window egui resource registration.
    pub fn update_uv_ui_resource(
        &mut self,
        egui_ctx: Option<egui::Context>,
        has_winit_state: bool,
        has_renderer: bool,
    ) {
        self.world.insert_resource(UvWindowUiResource {
            egui_ctx,
            has_winit_state,
            has_renderer,
        });
    }

    /// Register UI state into the world.
    pub fn register_ui_state(&mut self, ui_state: crate::app::ui::TransientUiState) {
        self.world.insert_resource(UiStateResource(ui_state));
    }

    /// Register GPU context into the world.
    pub fn register_gpu_context(
        &mut self,
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: std::sync::Arc<wgpu::Device>,
        queue: wgpu::Queue,
    ) {
        self.world.insert_resource(GpuContextResource {
            instance,
            adapter,
            device,
            queue,
        });
    }

    /// Register initial surface sizes.
    pub fn register_surface_registry(&mut self, main_width: u32, main_height: u32) {
        self.world.insert_resource(SurfaceRegistryResource {
            main_surface_size: (main_width.max(1), main_height.max(1)),
            uv_surface_size: None,
        });
    }

    /// Set UV surface size in the ECS registry.
    pub fn set_uv_surface_size(&mut self, width: u32, height: u32) {
        if let Some(mut registry) = self.world.get_resource_mut::<SurfaceRegistryResource>() {
            registry.uv_surface_size = Some((width.max(1), height.max(1)));
        }
    }

    /// Clear UV surface entry when viewer is closed.
    pub fn clear_uv_surface(&mut self) {
        if let Some(mut registry) = self.world.get_resource_mut::<SurfaceRegistryResource>() {
            registry.uv_surface_size = None;
        }
    }

    /// Phase 5.1: Set whether UV window UI resources are active.
    pub fn set_uv_ui_window_active(&mut self, active: bool) {
        if let Some(mut registry) = self.world.get_resource_mut::<UiWindowRegistryResource>() {
            registry.uv_window_ui_active = active;
        }
    }

    /// Send a window/surface lifecycle event into ECS.
    pub fn send_window_surface_event(&mut self, event: events::WindowSurfaceEvent) {
        self.world
            .get_resource_mut::<Events<events::WindowSurfaceEvent>>()
            .expect("WindowSurfaceEvent resource should be initialized")
            .send(event);
    }

    /// Send a render request event into ECS.
    pub fn send_render_request_event(&mut self, event: events::RenderRequestEvent) {
        self.world
            .get_resource_mut::<Events<events::RenderRequestEvent>>()
            .expect("RenderRequestEvent resource should be initialized")
            .send(event);
    }

    /// Send a redraw intent event from host window loop into ECS.
    pub fn send_redraw_event(&mut self, event: events::RedrawEvent) {
        self.world
            .get_resource_mut::<Events<events::RedrawEvent>>()
            .expect("RedrawEvent resource should be initialized")
            .send(event);
    }

    /// Send a render failure event for ECS-driven recovery.
    pub fn send_render_failure_event(&mut self, event: events::RenderFailureEvent) {
        self.world
            .get_resource_mut::<Events<events::RenderFailureEvent>>()
            .expect("RenderFailureEvent resource should be initialized")
            .send(event);
    }

    /// Take pending surface ops produced by ECS resize systems.
    pub fn take_pending_surface_ops(&mut self) -> PendingSurfaceOpsResource {
        let mut pending = PendingSurfaceOpsResource::default();
        if let Some(mut ops) = self.world.get_resource_mut::<PendingSurfaceOpsResource>() {
            pending = *ops;
            *ops = PendingSurfaceOpsResource::default();
        }
        pending
    }

    /// Take pending render operations produced by ECS systems.
    pub fn take_pending_render_ops(&mut self) -> PendingRenderOpsResource {
        let mut pending = PendingRenderOpsResource::default();
        if let Some(mut ops) = self.world.get_resource_mut::<PendingRenderOpsResource>() {
            pending = *ops;
            *ops = PendingRenderOpsResource::default();
        }
        pending
    }

    /// Take pending prepare-stage GPU operations produced by ECS systems.
    pub fn take_pending_prepare_ops(&mut self) -> PendingPrepareOpsResource {
        let mut pending = PendingPrepareOpsResource::default();
        if let Some(mut ops) = self.world.get_resource_mut::<PendingPrepareOpsResource>() {
            pending = *ops;
            *ops = PendingPrepareOpsResource::default();
        }
        pending
    }

    /// Take pending host-side domain operations emitted by ECS systems.
    pub fn take_pending_domain_host_ops(&mut self) -> PendingDomainHostOpsResource {
        let mut pending = PendingDomainHostOpsResource::default();
        if let Some(mut ops) = self.world.get_resource_mut::<PendingDomainHostOpsResource>() {
            pending = std::mem::take(&mut *ops);
        }
        pending
    }

    /// Take pending UI frame lifecycle operations produced by ECS systems.
    pub fn take_pending_ui_frame_ops(&mut self) -> PendingUiFrameOpsResource {
        let mut pending = PendingUiFrameOpsResource::default();
        if let Some(mut ops) = self.world.get_resource_mut::<PendingUiFrameOpsResource>() {
            pending = *ops;
            *ops = PendingUiFrameOpsResource::default();
        }
        pending
    }

    /// Send an AppEvent into the event queue to be processed next frame.
    pub fn send_event(&mut self, event: events::AppEvent) {
        self.world
            .get_resource_mut::<Events<events::AppEvent>>()
            .expect("AppEvent resource should be initialized")
            .send(event);
    }

    /// Tick the ECS schedule once per frame.
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }

    /// Drain all pending events from the queue and return them.
    pub fn drain_events(&mut self) -> Vec<events::AppEvent> {
        let mut events = Vec::new();
        if let Some(mut event_queue) = self.world.get_resource_mut::<Events<events::AppEvent>>() {
            for event in event_queue.drain() {
                events.push(event);
            }
        }
        events
    }

    /// Phase 3: Query the ordered system sets for scheduling.
    /// Returns the frame phases in execution order.
    pub fn get_frame_phases() -> Vec<FramePhase> {
        vec![
            FramePhase::WinitEventIngest,
            FramePhase::InputResolve,
            FramePhase::DomainUpdate,
            FramePhase::ExtractRenderData,
            FramePhase::PrepareGpu,
            FramePhase::RenderMainSurface,
            FramePhase::RenderAuxSurfaces,
            FramePhase::EndFrame,
        ]
    }

    /// Return a reference to the underlying World for system registration and resource access.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Return a mutable reference to the underlying World.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Return a mutable reference to the Schedule for system registration.
    pub fn schedule_mut(&mut self) -> &mut Schedule {
        &mut self.schedule
    }
}

impl Default for EcsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Systems module for ECS frame processing and event-driven runtime updates.
pub mod systems {
    use super::events::*;
    use super::{
        DomainStateResource,
        ExtractedCameraData,
        ExtractedDocumentData,
        ExtractedLayerComposition,
        PendingPrepareOpsResource,
        PendingDomainHostOpsResource,
        PendingRenderOpsResource,
        PendingSurfaceOpsResource,
        PendingUiFrameOpsResource,
        SurfaceRegistryResource,
        UiWindowRegistryResource,
        MainWindowUiResource,
        UvWindowUiResource,
    };
    use bevy_ecs::system::{Res, ResMut};

    /// Phase 4.1: Consume resize events and produce pending surface ops.
    pub fn window_surface_system(
        mut events: bevy_ecs::event::EventReader<WindowSurfaceEvent>,
        mut registry: ResMut<SurfaceRegistryResource>,
        mut pending_ops: ResMut<PendingSurfaceOpsResource>,
    ) {
        for event in events.read() {
            match event {
                WindowSurfaceEvent::MainWindowResized { width, height } => {
                    let size = ((*width).max(1), (*height).max(1));
                    registry.main_surface_size = size;
                    pending_ops.main_resize = Some(size);
                }
                WindowSurfaceEvent::UvWindowResized { width, height } => {
                    let size = ((*width).max(1), (*height).max(1));
                    registry.uv_surface_size = Some(size);
                    pending_ops.uv_resize = Some(size);
                }
            }
        }
    }

    /// Ingest redraw intents and translate them into render requests.
    pub fn redraw_ingest_system(
        mut redraw_events: bevy_ecs::event::EventReader<RedrawEvent>,
        mut render_requests: ResMut<bevy_ecs::event::Events<RenderRequestEvent>>,
    ) {
        for event in redraw_events.read() {
            match event {
                RedrawEvent::MainSurface => {
                    render_requests.send(RenderRequestEvent::MainSurface);
                }
                RedrawEvent::UvSurface => {
                    render_requests.send(RenderRequestEvent::UvSurface);
                }
            }
        }
    }

    /// Phase 4.2: Prepare GPU-stage work before render passes.
    pub fn prepare_gpu_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_prepare_ops: ResMut<PendingPrepareOpsResource>,
    ) {
        for event in events.read() {
            if matches!(event, RenderRequestEvent::MainSurface) {
                pending_prepare_ops.update_main_camera_uniform = true;
            }
        }
    }

    /// Phase 4.2: Render 3D viewport request system.
    pub fn render_3d_viewport_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_ops: ResMut<PendingRenderOpsResource>,
    ) {
        for event in events.read() {
            if matches!(event, RenderRequestEvent::MainSurface) {
                pending_ops.render_main_surface = true;
                pending_ops.render_3d_viewport_pass = true;
            }
        }
    }

    /// Phase 4.2: Paint composition request system.
    /// For now this shares the main surface render trigger to preserve existing flow.
    pub fn render_paint_composite_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_ops: ResMut<PendingRenderOpsResource>,
    ) {
        for event in events.read() {
            if matches!(event, RenderRequestEvent::MainSurface) {
                pending_ops.render_main_surface = true;
                pending_ops.render_paint_composite_pass = true;
            }
        }
    }

    /// Phase 4.2: Auxiliary surface render request system.
    pub fn render_aux_surface_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_ops: ResMut<PendingRenderOpsResource>,
    ) {
        for event in events.read() {
            if matches!(event, RenderRequestEvent::UvSurface) {
                pending_ops.render_uv_surface = true;
            }
        }
    }

    /// Phase 4.2: Recover lost/outdated surfaces by scheduling reconfigure operations.
    pub fn render_recovery_system(
        mut failures: bevy_ecs::event::EventReader<RenderFailureEvent>,
        registry: Res<SurfaceRegistryResource>,
        mut pending_surface_ops: ResMut<PendingSurfaceOpsResource>,
    ) {
        for failure in failures.read() {
            match (failure.surface, failure.kind) {
                (RenderSurfaceKind::Main, RenderFailureKind::Lost)
                | (RenderSurfaceKind::Main, RenderFailureKind::Outdated) => {
                    pending_surface_ops.main_resize = Some(registry.main_surface_size);
                }
                (RenderSurfaceKind::Uv, RenderFailureKind::Lost)
                | (RenderSurfaceKind::Uv, RenderFailureKind::Outdated) => {
                    if let Some(size) = registry.uv_surface_size {
                        pending_surface_ops.uv_resize = Some(size);
                    }
                }
            }
        }
    }

    /// Phase 5.1: Begin egui frame lifecycle stage.
    pub fn begin_egui_frame_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        ui_windows: Res<UiWindowRegistryResource>,
        main_ui: Option<Res<MainWindowUiResource>>,
        uv_ui: Option<Res<UvWindowUiResource>>,
        mut pending_ui_ops: ResMut<PendingUiFrameOpsResource>,
    ) {
        let main_ready = main_ui
            .map(|r| r.has_winit_state && r.has_renderer)
            .unwrap_or(false);
        let uv_ready = uv_ui
            .map(|r| r.has_winit_state && r.has_renderer && r.egui_ctx.is_some())
            .unwrap_or(false);

        for event in events.read() {
            match event {
                RenderRequestEvent::MainSurface => {
                    if ui_windows.main_window_ui_active && main_ready {
                        pending_ui_ops.begin_main_egui_frame = true;
                    }
                }
                RenderRequestEvent::UvSurface => {
                    if ui_windows.uv_window_ui_active && uv_ready {
                        pending_ui_ops.begin_uv_egui_frame = true;
                    }
                }
            }
        }
    }

    /// Phase 5.1: Draw egui panels lifecycle stage.
    pub fn draw_egui_panels_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        ui_windows: Res<UiWindowRegistryResource>,
        main_ui: Option<Res<MainWindowUiResource>>,
        uv_ui: Option<Res<UvWindowUiResource>>,
        mut pending_ui_ops: ResMut<PendingUiFrameOpsResource>,
    ) {
        let main_ready = main_ui
            .map(|r| r.has_winit_state && r.has_renderer)
            .unwrap_or(false);
        let uv_ready = uv_ui
            .map(|r| r.has_winit_state && r.has_renderer && r.egui_ctx.is_some())
            .unwrap_or(false);

        for event in events.read() {
            match event {
                RenderRequestEvent::MainSurface => {
                    if ui_windows.main_window_ui_active && main_ready {
                        pending_ui_ops.draw_main_egui_panels = true;
                    }
                }
                RenderRequestEvent::UvSurface => {
                    if ui_windows.uv_window_ui_active && uv_ready {
                        pending_ui_ops.draw_uv_egui_panels = true;
                    }
                }
            }
        }
    }

    /// Phase 5.1: End egui frame + upload lifecycle stage.
    pub fn end_egui_frame_and_upload_system(
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        ui_windows: Res<UiWindowRegistryResource>,
        main_ui: Option<Res<MainWindowUiResource>>,
        uv_ui: Option<Res<UvWindowUiResource>>,
        mut pending_ui_ops: ResMut<PendingUiFrameOpsResource>,
    ) {
        let main_ready = main_ui
            .map(|r| r.has_winit_state && r.has_renderer)
            .unwrap_or(false);
        let uv_ready = uv_ui
            .map(|r| r.has_winit_state && r.has_renderer && r.egui_ctx.is_some())
            .unwrap_or(false);

        for event in events.read() {
            match event {
                RenderRequestEvent::MainSurface => {
                    if ui_windows.main_window_ui_active && main_ready {
                        pending_ui_ops.end_main_egui_frame_and_upload = true;
                    }
                }
                RenderRequestEvent::UvSurface => {
                    if ui_windows.uv_window_ui_active && uv_ready {
                        pending_ui_ops.end_uv_egui_frame_and_upload = true;
                    }
                }
            }
        }
    }

    /// Consume viewport command events and queue host-side adapters.
    pub fn viewport_command_system(
        mut events: bevy_ecs::event::EventReader<AppEvent>,
        mut pending_host_ops: ResMut<PendingDomainHostOpsResource>,
    ) {
        for event in events.read() {
            if let AppEvent::Viewport(command) = event {
                pending_host_ops.viewport_commands.push(*command);
            }
        }
    }

    /// Consume tool command events and queue host-side adapters.
    pub fn tool_command_system(
        mut events: bevy_ecs::event::EventReader<AppEvent>,
        mut pending_host_ops: ResMut<PendingDomainHostOpsResource>,
    ) {
        for event in events.read() {
            if let AppEvent::Tool(command) = event {
                pending_host_ops.tool_commands.push(*command);
            }
        }
    }

    /// Consume input state commands, mirror ECS domain state, and queue host adapters.
    pub fn input_state_system(
        mut events: bevy_ecs::event::EventReader<AppEvent>,
        mut domain: ResMut<DomainStateResource>,
        mut pending_host_ops: ResMut<PendingDomainHostOpsResource>,
    ) {
        for event in events.read() {
            if let AppEvent::InputState(command) = event {
                crate::app::ecs::domain::apply_input_state_event_to_app_state(&mut domain.0, command);
                pending_host_ops.input_state_commands.push(*command);
            }
        }
    }

    /// System stub: consumes UI action events (actual dispatch happens in State).
    pub fn ui_action_system(
        mut events: bevy_ecs::event::EventReader<AppEvent>,
        mut domain: ResMut<DomainStateResource>,
        mut pending_host_ops: ResMut<PendingDomainHostOpsResource>,
    ) {
        for event in events.read() {
            if let AppEvent::Ui(ui_action) = event {
                crate::app::ecs::domain::apply_ui_event_to_app_state(&mut domain.0, ui_action);
                pending_host_ops.ui_actions.push(ui_action.clone());
            }
        }
    }

    /// System stub: consumes document command events (actual dispatch happens in State).
    pub fn document_command_system(
        mut events: bevy_ecs::event::EventReader<AppEvent>,
        mut domain: ResMut<DomainStateResource>,
        mut pending_host_ops: ResMut<PendingDomainHostOpsResource>,
    ) {
        for event in events.read() {
            if let AppEvent::Document(command) = event {
                crate::app::ecs::domain::apply_document_event_to_app_state(&mut domain.0, command);
                pending_host_ops.document_commands.push(command.clone());
            }
        }
    }

    /// Phase 3.2: Extract camera state for read-only render access.
    /// Runs in ExtractRenderData phase; reads DomainStateResource, writes ExtractedCameraData.
    /// (Stub: full viewport data will be added when viewport moves to ECS resources)
    pub fn extract_camera_system(
        _domain: Res<DomainStateResource>,
        mut extracted_camera: ResMut<ExtractedCameraData>,
    ) {
        // For now, just keep the extracted data as-is.
        // In Phase 4, viewport will be registered as an ECS resource.
        *extracted_camera = ExtractedCameraData::default();
    }

    /// Phase 3.2: Extract layer composition for read-only render access.
    /// Runs in ExtractRenderData phase; reads DomainStateResource, writes ExtractedLayerComposition.
    pub fn extract_layer_composition_system(
        domain: Res<DomainStateResource>,
        mut extracted_layers: ResMut<ExtractedLayerComposition>,
    ) {
        let app_state = &domain.0;
        let layer_count = app_state.document().layer_count;
        
        extracted_layers.layer_count = layer_count;
        extracted_layers.active_layer_idx = app_state.document().active_layer_idx;
        
        // Populate per-layer visibility and opacity (stub for now)
        extracted_layers.layer_visibility = vec![true; layer_count];
        extracted_layers.layer_opacities = vec![1.0; layer_count];
    }

    /// Phase 3.2: Extract document metadata for read-only render access.
    /// Runs in ExtractRenderData phase; reads DomainStateResource, writes ExtractedDocumentData.
    pub fn extract_document_data_system(
        domain: Res<DomainStateResource>,
        mut extracted_doc: ResMut<ExtractedDocumentData>,
    ) {
        let app_state = &domain.0;
        extracted_doc.current_mesh = app_state.document().current_mesh.clone();
        extracted_doc.num_udim_tiles = app_state.document().num_udim_tiles as usize;
        extracted_doc.num_paint_layers = app_state.document().layer_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecs_runtime_creates() {
        let _runtime = EcsRuntime::new();
        // Runtime created successfully
    }

    #[test]
    fn test_ecs_runtime_ticks() {
        let mut runtime = EcsRuntime::new();
        runtime.tick(); // Should not panic
    }

    #[test]
    fn test_send_event() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Viewport(
            events::ViewportCommandEvent::Zoom { scroll: 1.0 },
        ));
        runtime.tick();
        // Should not panic
    }

    #[test]
    fn test_drain_events_preserves_send_order() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Viewport(
            events::ViewportCommandEvent::Zoom { scroll: 1.0 },
        ));
        runtime.send_event(events::AppEvent::Document(
            events::DocumentCommandEvent::CommitCurrentStroke,
        ));

        let drained = runtime.drain_events();
        assert_eq!(drained.len(), 2);

        assert!(matches!(
            drained[0],
            events::AppEvent::Viewport(events::ViewportCommandEvent::Zoom { .. })
        ));
        assert!(matches!(
            drained[1],
            events::AppEvent::Document(events::DocumentCommandEvent::CommitCurrentStroke)
        ));
    }

    #[test]
    fn test_tool_runtime_resource_initialized() {
        let runtime = EcsRuntime::new();
        assert!(runtime.world.get_resource::<ToolRuntimeResource>().is_some());
    }

    #[test]
    fn test_window_surface_event_generates_pending_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_window_surface_event(events::WindowSurfaceEvent::MainWindowResized {
            width: 1280,
            height: 720,
        });
        runtime.tick();

        let ops = runtime.take_pending_surface_ops();
        assert_eq!(ops.main_resize, Some((1280, 720)));
    }

    #[test]
    fn test_render_request_generates_pending_render_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_render_request_event(events::RenderRequestEvent::MainSurface);
        runtime.tick();

        let ops = runtime.take_pending_render_ops();
        assert!(ops.render_main_surface);
        assert!(ops.render_3d_viewport_pass);
        assert!(ops.render_paint_composite_pass);
    }

    #[test]
    fn test_redraw_event_generates_pending_render_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_redraw_event(events::RedrawEvent::MainSurface);
        runtime.tick();

        let ops = runtime.take_pending_render_ops();
        assert!(ops.render_main_surface);
    }

    #[test]
    fn test_prepare_gpu_generates_pending_prepare_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_render_request_event(events::RenderRequestEvent::MainSurface);
        runtime.tick();

        let ops = runtime.take_pending_prepare_ops();
        assert!(ops.update_main_camera_uniform);
    }

    #[test]
    fn test_ui_lifecycle_ops_generated_for_main_surface() {
        let mut runtime = EcsRuntime::new();
        runtime.register_main_ui_resource(egui::Context::default(), true, true);
        runtime.send_render_request_event(events::RenderRequestEvent::MainSurface);
        runtime.tick();

        let ui_ops = runtime.take_pending_ui_frame_ops();
        assert!(ui_ops.begin_main_egui_frame);
        assert!(ui_ops.draw_main_egui_panels);
        assert!(ui_ops.end_main_egui_frame_and_upload);
    }

    #[test]
    fn test_render_failure_generates_recovery_resize_op() {
        let mut runtime = EcsRuntime::new();
        runtime.register_surface_registry(1280, 720);
        runtime.send_render_failure_event(events::RenderFailureEvent {
            surface: events::RenderSurfaceKind::Main,
            kind: events::RenderFailureKind::Lost,
        });
        runtime.tick();

        let ops = runtime.take_pending_surface_ops();
        assert_eq!(ops.main_resize, Some((1280, 720)));
    }

    #[test]
    fn test_frame_phases_ordered() {
        let phases = EcsRuntime::get_frame_phases();
        // Verify all 8 phases are present and in correct order
        assert_eq!(phases.len(), 8);
        assert_eq!(phases[0], FramePhase::WinitEventIngest);
        assert_eq!(phases[1], FramePhase::InputResolve);
        assert_eq!(phases[2], FramePhase::DomainUpdate);
        assert_eq!(phases[3], FramePhase::ExtractRenderData);
        assert_eq!(phases[4], FramePhase::PrepareGpu);
        assert_eq!(phases[5], FramePhase::RenderMainSurface);
        assert_eq!(phases[6], FramePhase::RenderAuxSurfaces);
        assert_eq!(phases[7], FramePhase::EndFrame);
    }

    #[test]
    fn test_extracted_render_data_initialized() {
        let runtime = EcsRuntime::new();
        // Verify extracted resources are initialized
        assert!(runtime.world.get_resource::<ExtractedCameraData>().is_some());
        assert!(runtime.world.get_resource::<ExtractedLayerComposition>().is_some());
        assert!(runtime.world.get_resource::<ExtractedDocumentData>().is_some());
    }

    #[test]
    fn test_extracted_document_data_reflects_synced_domain_state() {
        let mut runtime = EcsRuntime::new();
        let mut app_state = crate::app::app_state::AppState::new();

        app_state.document_mut().current_mesh = "Cube".to_string();
        app_state.document_mut().num_udim_tiles = 7;
        app_state.document_mut().layer_count = 3;

        runtime.sync_domain_state_from(&app_state);
        runtime.tick();

        let extracted = runtime
            .world
            .get_resource::<ExtractedDocumentData>()
            .expect("ExtractedDocumentData must exist");

        assert_eq!(extracted.current_mesh, "Cube");
        assert_eq!(extracted.num_udim_tiles, 7);
        assert_eq!(extracted.num_paint_layers, 3);
    }

    #[test]
    fn test_ui_system_mutates_domain_state_resource() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Ui(events::UiActionEvent::SetBrushSize(123.0)));

        runtime.tick();

        let domain = runtime
            .world
            .get_resource::<DomainStateResource>()
            .expect("DomainStateResource must exist");
        let mut snapshot = domain.0.clone();
        assert_eq!(snapshot.canvas_mut().brush_size, 123.0);
    }

    #[test]
    fn test_document_system_mutates_domain_state_resource() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Document(
            events::DocumentCommandEvent::ClearAllLayers,
        ));

        runtime.tick();

        let domain = runtime
            .world
            .get_resource::<DomainStateResource>()
            .expect("DomainStateResource must exist");
        let mut snapshot = domain.0.clone();
        assert_eq!(snapshot.document_mut().layer_count, 1);
        assert_eq!(snapshot.document_mut().active_layer_idx, 0);
    }

    #[test]
    fn test_ui_system_emits_pending_host_ui_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Ui(events::UiActionEvent::SetBrushOpacity(
            0.25,
        )));

        runtime.tick();

        let pending = runtime.take_pending_domain_host_ops();
        assert_eq!(pending.ui_actions.len(), 1);
        assert!(matches!(
            pending.ui_actions[0],
            events::UiActionEvent::SetBrushOpacity(_)
        ));
    }

    #[test]
    fn test_document_system_emits_pending_host_document_ops() {
        let mut runtime = EcsRuntime::new();
        runtime.send_event(events::AppEvent::Document(
            events::DocumentCommandEvent::CommitCurrentStroke,
        ));

        runtime.tick();

        let pending = runtime.take_pending_domain_host_ops();
        assert_eq!(pending.document_commands.len(), 1);
        assert!(matches!(
            pending.document_commands[0],
            events::DocumentCommandEvent::CommitCurrentStroke
        ));
    }
}
