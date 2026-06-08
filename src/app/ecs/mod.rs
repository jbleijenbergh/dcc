//! ECS runtime layer for the DCC application.
//!
//! This module provides a Bevy ECS + App infrastructure that integrates
//! the winit event loop and wgpu rendering pipeline with ECS-driven
//! state management and scheduling.

use bevy_ecs::prelude::*;

pub mod domain;
mod plugins;

// --- Layer Components ---
#[derive(Component, Clone, Debug)]
pub struct LayerName(pub String);

#[derive(Component, Clone, Copy, Debug)]
pub struct LayerOpacity(pub f32);

#[derive(Component, Clone, Copy, Debug)]
pub struct LayerVisibility(pub bool);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerBlendMode(pub crate::painter::BlendMode);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayerIndex(pub usize);

#[derive(Component, Clone, Debug)]
pub struct LayerTexture {
    pub texture: std::sync::Arc<wgpu::Texture>,
    pub views: Vec<std::sync::Arc<wgpu::TextureView>>, // size 4 (UDIMs)
}

#[derive(Component, Clone, Debug, Default)]
pub struct LayerStrokes(pub Vec<crate::painter::PaintStroke>);

#[derive(Component, Clone, Debug)]
pub struct FillLayerProperties {
    pub color: [u8; 4],
    pub noise_color: [u8; 4],
    pub noise_scale: f32,
    pub projection_mode: u32,
}

#[derive(Component, Clone, Debug)]
pub struct ActiveLayer;

// --- 3D Mesh Components ---
#[derive(Component, Clone, Copy, Debug)]
pub struct Transform {
    pub translation: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn to_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

#[derive(Component, Clone)]
pub struct MeshHandle(pub std::sync::Arc<crate::mesh::Mesh>);

#[derive(Component, Clone, Copy, Debug)]
pub struct Aabb {
    pub min: glam::Vec3,
    pub max: glam::Vec3,
}

#[derive(Component)]
pub struct NodeGpuResources {
    pub uniform_buffer: std::sync::Arc<wgpu::Buffer>,
    pub bind_group: std::sync::Arc<wgpu::BindGroup>,
}

// --- Camera & Viewport Resources ---
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraResource {
    pub target: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Default for CameraResource {
    fn default() -> Self {
        Self {
            target: glam::Vec3::ZERO,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.25,
            distance: 4.5,
            aspect: 1.0,
            fovy: std::f32::consts::FRAC_PI_4,
            znear: 0.1,
            zfar: 100.0,
        }
    }
}

impl CameraResource {
    pub fn get_eye(&self) -> glam::Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.cos() + self.target.x;
        let y = self.distance * self.pitch.sin() + self.target.y;
        let z = self.distance * self.pitch.cos() * self.yaw.sin() + self.target.z;
        glam::Vec3::new(x, y, z)
    }

    pub fn build_view_projection_matrix(&self) -> glam::Mat4 {
        let eye = self.get_eye();
        let view = glam::Mat4::look_at_rh(eye, self.target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }
}

#[derive(Resource)]
pub struct CameraGpuResources {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportSettingsResource {
    pub light_angle: f32,
    pub light_intensity: f32,
    pub ambient_strength: f32,
    pub view_transform: crate::viewport::ViewTransform,
    pub exposure: f32,
}

impl Default for ViewportSettingsResource {
    fn default() -> Self {
        Self {
            light_angle: 0.0,
            light_intensity: 1.0,
            ambient_strength: 0.25,
            view_transform: crate::viewport::ViewTransform::Standard,
            exposure: 1.0,
        }
    }
}

#[derive(Resource)]
pub struct DocumentResource {
    pub document: crate::mesh::Document,
}

#[derive(Resource)]
pub struct PainterResource(pub crate::painter::Painter);

pub fn create_layer_texture(device: &wgpu::Device, width: u32, height: u32) -> LayerTexture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: crate::painter::MAX_UDIMS as u32,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ECS Layer Texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let mut views = Vec::new();
    for i in 0..crate::painter::MAX_UDIMS {
        views.push(std::sync::Arc::new(texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("ECS Layer View Tile {}", i)),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: i as u32,
            array_layer_count: Some(1),
            ..Default::default()
        })));
    }

    LayerTexture {
        texture: std::sync::Arc::new(texture),
        views,
    }
}


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
    use crate::app::input::{ModifiersSnapshot, PointerData};
    use crate::painter::BlendMode;
    use bevy_ecs::event::Event;

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
        SetPressureCurve {
            min_start: f32,
            max_at: f32,
        },
        StartGltfLoad,
        FinishGltfLoadSuccess {
            filename: String,
        },
        FinishGltfLoadError {
            path: std::path::PathBuf,
            message: String,
        },
        DismissLoadError,
        SelectLayer(usize),
        AddPaintLayer(String),
        AddUvGridLayer,
        AddUvCheckerLayer,
        AddFillLayer,
        DeleteLayer(usize),
        SetLayerVisible {
            idx: usize,
            visible: bool,
        },
        SetLayerBlendMode {
            idx: usize,
            mode: BlendMode,
        },
        SetLayerOpacity {
            idx: usize,
            opacity: f32,
            begin_undo: bool,
        },
        SetFillBaseColor {
            idx: usize,
            color: [u8; 4],
            begin_undo: bool,
        },
        SetFillNoiseColor {
            idx: usize,
            color: [u8; 4],
            begin_undo: bool,
        },
        SetFillNoiseScale {
            idx: usize,
            scale: f32,
            begin_undo: bool,
        },
        SetFillProjectionMode {
            idx: usize,
            mode: u32,
        },
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
        UpdateModifier {
            key: winit::keyboard::KeyCode,
            is_pressed: bool,
        },
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

/// ECS resource owning the main window winit handle.
#[derive(Resource, Clone)]
pub struct WindowResource(pub std::sync::Arc<winit::window::Window>);

/// ECS resource owning the main surface and depth target rendering context.
#[derive(Resource)]
pub struct MainRenderContextResource {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

/// Resource wrapper for user preferences and settings.
#[derive(Resource, Clone, Default)]
pub struct PreferencesResource(pub crate::app::user_preferences::UserPreferences);

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

#[derive(Resource, Copy, Clone)]
pub struct HostStatePtr(pub *mut crate::app::State);

unsafe impl Send for HostStatePtr {}
unsafe impl Sync for HostStatePtr {}

#[derive(Resource, Default)]
pub struct RenderErrorResource(pub Option<crate::app::SurfaceError>);

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
        world.insert_resource(DomainStateResource(crate::app::app_state::AppState::new()));
        world.init_resource::<SurfaceRegistryResource>();
        world.init_resource::<PendingSurfaceOpsResource>();
        world.init_resource::<PendingPrepareOpsResource>();
        world.init_resource::<PendingDomainHostOpsResource>();
        world.init_resource::<PendingRenderOpsResource>();
        world.init_resource::<UiWindowRegistryResource>();
        world.init_resource::<PendingUiFrameOpsResource>();
        world.init_resource::<ToolRuntimeResource>();
        world.insert_resource(HostStatePtr(std::ptr::null_mut()));
        world.init_resource::<RenderErrorResource>();
        world.init_resource::<CameraResource>();
        world.init_resource::<ViewportSettingsResource>();
        world.init_resource::<InteractionStateResource>();
        world.init_resource::<PreferencesResource>();

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

    /// Register a Document by spawning its meshes as ECS entities and setting the DocumentResource.
    pub fn register_document(&mut self, document: crate::mesh::Document) {
        // Extract active nodes from a cloned document so we don't consume the original
        let active_nodes = document.clone().into_active_nodes();

        let mut world = self.world_mut();

        // 1. Despawn existing mesh entities
        let old_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<MeshHandle>>()
            .iter(world)
            .collect();
        for entity in old_entities {
            world.despawn(entity);
        }

        // 2. Spawn entities for active nodes
        for (_name, world_matrix, mesh, uniform_buffer, bind_group) in active_nodes {
            let (scale, rotation, translation) = world_matrix.to_scale_rotation_translation();

            let mut bounds_min = glam::Vec3::splat(f32::MAX);
            let mut bounds_max = glam::Vec3::splat(f32::MIN);
            for primitive in &mesh.primitives {
                bounds_min = bounds_min.min(primitive.bounds_min);
                bounds_max = bounds_max.max(primitive.bounds_max);
            }
            let aabb = Aabb {
                min: bounds_min,
                max: bounds_max,
            };

            world.spawn((
                Transform {
                    translation,
                    rotation,
                    scale,
                },
                MeshHandle(std::sync::Arc::new(mesh)),
                aabb,
                NodeGpuResources {
                    uniform_buffer,
                    bind_group,
                },
            ));
        }

        // 3. Insert/update DocumentResource
        world.insert_resource(DocumentResource {
            document,
        });
    }

    pub fn init_default_layer(&mut self, device: &wgpu::Device) {
        let texture = create_layer_texture(device, 1024, 1024);
        let mut world = self.world_mut();

        world.spawn((
            LayerName("Layer 1".to_string()),
            LayerOpacity(1.0),
            LayerVisibility(true),
            LayerBlendMode(crate::painter::BlendMode::Normal),
            LayerIndex(0),
            texture,
            LayerStrokes(Vec::new()),
            ActiveLayer,
        ));
    }

    pub fn clear_all_layers_ecs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut world = self.world_mut();
        let layers: Vec<Entity> = world
            .query_filtered::<Entity, With<LayerName>>()
            .iter(world)
            .collect();
        for entity in layers {
            world.despawn(entity);
        }
        self.init_default_layer(device);
        self.redraw_all_layers_ecs(device, queue);
    }

    pub fn load_uv_grid_layer_ecs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let trimmed = "UV Grid";
        let mut world = self.world_mut();
        
        let next_idx = {
            let mut query = world.query::<&LayerIndex>();
            query.iter(world).map(|idx| idx.0).max().map(|max| max + 1).unwrap_or(0)
        };

        let mut active_entities = Vec::new();
        let mut active_query = world.query_filtered::<Entity, With<ActiveLayer>>();
        for entity in active_query.iter(world) {
            active_entities.push(entity);
        }
        for entity in active_entities {
            world.entity_mut(entity).remove::<ActiveLayer>();
        }

        let texture = create_layer_texture(device, 1024, 1024);

        let img = match image::open("uv_grid.png") {
            Ok(img) => img.into_rgba8(),
            Err(e) => {
                log::error!("Failed to load uv_grid.png: {}", e);
                return;
            }
        };

        for i in 0..crate::painter::MAX_UDIMS {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &img,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * 1024),
                    rows_per_image: Some(1024),
                },
                wgpu::Extent3d {
                    width: 1024,
                    height: 1024,
                    depth_or_array_layers: 1,
                },
            );
        }

        world.spawn((
            LayerName(trimmed.to_string()),
            LayerOpacity(1.0),
            LayerVisibility(true),
            LayerBlendMode(crate::painter::BlendMode::Normal),
            LayerIndex(next_idx),
            texture,
            LayerStrokes(Vec::new()),
            ActiveLayer,
        ));

        self.redraw_all_layers_ecs(device, queue);
    }

    pub fn load_uv_checker_layer_ecs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let trimmed = "UV Checker";
        let mut world = self.world_mut();
        
        let next_idx = {
            let mut query = world.query::<&LayerIndex>();
            query.iter(world).map(|idx| idx.0).max().map(|max| max + 1).unwrap_or(0)
        };

        let mut active_entities = Vec::new();
        let mut active_query = world.query_filtered::<Entity, With<ActiveLayer>>();
        for entity in active_query.iter(world) {
            active_entities.push(entity);
        }
        for entity in active_entities {
            world.entity_mut(entity).remove::<ActiveLayer>();
        }

        let texture = create_layer_texture(device, 1024, 1024);

        for t in 0..crate::painter::MAX_UDIMS {
            let filename = format!("UV-CheckerMap_Maurus_0{}_8K.png", t + 1);
            let img = match image::open(&filename) {
                Ok(img) => img.into_rgba8(),
                Err(e) => {
                    log::error!("Failed to load {}: {}", filename, e);
                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    {
                        let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Clear Missing Checker Tile"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &texture.views[t],
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                    }
                    queue.submit(std::iter::once(encoder.finish()));
                    continue;
                }
            };

            let resized_img = if img.width() != 1024 || img.height() != 1024 {
                image::imageops::resize(&img, 1024, 1024, image::imageops::FilterType::Triangle)
            } else {
                img
            };

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: t as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &resized_img,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * 1024),
                    rows_per_image: Some(1024),
                },
                wgpu::Extent3d {
                    width: 1024,
                    height: 1024,
                    depth_or_array_layers: 1,
                },
            );
        }

        world.spawn((
            LayerName(trimmed.to_string()),
            LayerOpacity(1.0),
            LayerVisibility(true),
            LayerBlendMode(crate::painter::BlendMode::Normal),
            LayerIndex(next_idx),
            texture,
            LayerStrokes(Vec::new()),
            ActiveLayer,
        ));

        self.redraw_all_layers_ecs(device, queue);
    }

    pub fn add_fill_layer_ecs(&mut self, name: String, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut world = self.world_mut();
        
        let next_idx = {
            let mut query = world.query::<&LayerIndex>();
            query.iter(world).map(|idx| idx.0).max().map(|max| max + 1).unwrap_or(0)
        };

        let mut active_entities = Vec::new();
        let mut active_query = world.query_filtered::<Entity, With<ActiveLayer>>();
        for entity in active_query.iter(world) {
            active_entities.push(entity);
        }
        for entity in active_entities {
            world.entity_mut(entity).remove::<ActiveLayer>();
        }

        let texture = create_layer_texture(device, 1024, 1024);

        world.spawn((
            LayerName(name),
            LayerOpacity(1.0),
            LayerVisibility(true),
            LayerBlendMode(crate::painter::BlendMode::Normal),
            LayerIndex(next_idx),
            texture,
            LayerStrokes(Vec::new()),
            FillLayerProperties {
                color: [128, 128, 128, 255],
                noise_color: [255, 255, 255, 255],
                noise_scale: 10.0,
                projection_mode: 0,
            },
            ActiveLayer,
        ));

        self.redraw_all_layers_ecs(device, queue);
    }

    pub fn delete_layer_ecs(&mut self, index: usize, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut world = self.world_mut();
        
        let mut entity_to_delete = None;
        let mut query = world.query::<(Entity, &LayerIndex)>();
        for (entity, idx) in query.iter(world) {
            if idx.0 == index {
                entity_to_delete = Some(entity);
            }
        }

        let was_active = if let Some(ent) = entity_to_delete {
            let active = world.entity(ent).contains::<ActiveLayer>();
            world.despawn(ent);
            active
        } else {
            return;
        };

        let mut idx_query = world.query::<(Entity, &mut LayerIndex)>();
        for (_, mut idx) in idx_query.iter_mut(world) {
            if idx.0 > index {
                idx.0 -= 1;
            }
        }

        if was_active {
            let mut max_idx = 0;
            let mut max_entity = None;
            let mut query = world.query::<(Entity, &LayerIndex)>();
            for (entity, idx) in query.iter(world) {
                if idx.0 >= max_idx {
                    max_idx = idx.0;
                    max_entity = Some(entity);
                }
            }
            let target_active_idx = index.min(max_idx);
            let mut query = world.query::<(Entity, &LayerIndex)>();
            for (entity, idx) in query.iter(world) {
                if idx.0 == target_active_idx {
                    world.entity_mut(entity).insert(ActiveLayer);
                    break;
                }
            }
        }

        self.redraw_all_layers_ecs(device, queue);
    }

    pub fn redraw_all_layers_ecs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut world = self.world_mut();

        let mut nodes = Vec::new();
        {
            let mut mesh_query = world.query::<(&MeshHandle, &NodeGpuResources)>();
            for (mesh, gpu_res) in mesh_query.iter(world) {
                nodes.push((mesh.0.clone(), gpu_res.bind_group.clone()));
            }
        }

        let mut painter_res = world.remove_resource::<PainterResource>().expect("PainterResource");
        let painter = &painter_res.0;

        let doc_res = world.get_resource::<DocumentResource>().expect("DocumentResource");
        let num_udim_tiles = doc_res.document.num_udim_tiles;

        let mut layers_query = world.query::<(
            &LayerIndex,
            &LayerTexture,
            &LayerStrokes,
            Option<&FillLayerProperties>,
        )>();
        let mut layers: Vec<_> = layers_query.iter(world).collect();
        layers.sort_by_key(|(idx, _, _, _)| idx.0);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Clear ECS Layers") });
        for (_, texture_comp, _, _) in &layers {
            for t in 0..crate::painter::MAX_UDIMS {
                let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear ECS Layer Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &texture_comp.views[t],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
            }
        }
        queue.submit(std::iter::once(encoder.finish()));

        for (_, texture_comp, strokes, fill_props) in &layers {
            let view_refs: Vec<&wgpu::TextureView> = texture_comp.views.iter().map(|v| &**v).collect();
            if let Some(fill) = fill_props {
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
                let node_refs: Vec<(&crate::mesh::Mesh, &wgpu::BindGroup)> = nodes
                    .iter()
                    .map(|(m, bg)| (&**m, &**bg))
                    .collect();
                painter.render_fill_layer_to_views(
                    device,
                    queue,
                    &view_refs,
                    base,
                    noise,
                    fill.noise_scale,
                    fill.projection_mode,
                    &node_refs,
                );
            } else {
                for stroke in &strokes.0 {
                    painter.paint_stroke_udim_to_views(
                        device,
                        queue,
                        &view_refs,
                        stroke,
                        num_udim_tiles,
                    );
                }
            }
        }

        world.insert_resource(painter_res);

        let mut compose_query = world.query::<(&LayerIndex, &LayerVisibility, &LayerOpacity, &LayerBlendMode)>();
        let mut compose_layers: Vec<_> = compose_query.iter(world).collect();
        compose_layers.sort_by_key(|(idx, _, _, _)| idx.0);

        let active_layers_data: Vec<(f32, crate::painter::BlendMode, bool)> = compose_layers
            .iter()
            .map(|(_, vis, opacity, blend)| (opacity.0, blend.0, vis.0))
            .collect();

        let painter_res = world.get_resource::<PainterResource>().unwrap();
        painter_res.0.compose_layers_ecs(device, queue, &active_layers_data);
    }

    pub fn compose_layers_only_ecs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut world = self.world_mut();
        let mut compose_query = world.query::<(&LayerIndex, &LayerVisibility, &LayerOpacity, &LayerBlendMode)>();
        let mut compose_layers: Vec<_> = compose_query.iter(world).collect();
        compose_layers.sort_by_key(|(idx, _, _, _)| idx.0);

        let active_layers_data: Vec<(f32, crate::painter::BlendMode, bool)> = compose_layers
            .iter()
            .map(|(_, vis, opacity, blend)| (opacity.0, blend.0, vis.0))
            .collect();

        let painter_res = world.get_resource::<PainterResource>().unwrap();
        painter_res.0.compose_layers_ecs(device, queue, &active_layers_data);
    }

    pub fn get_layers_snapshot(&mut self) -> Vec<crate::painter::Layer> {
        let mut world = self.world_mut();
        let mut query = world.query::<(
            &LayerIndex,
            &LayerName,
            &LayerOpacity,
            &LayerVisibility,
            &LayerBlendMode,
            &LayerStrokes,
            Option<&FillLayerProperties>,
        )>();
        let mut layers: Vec<_> = query
            .iter(world)
            .map(|(idx, name, opacity, vis, blend, strokes, fill)| {
                let is_fill = fill.is_some();
                let (fill_color, fill_noise_color, fill_noise_scale, fill_projection_mode) = if let Some(f) = fill {
                    (f.color, f.noise_color, f.noise_scale, f.projection_mode)
                } else {
                    ([128, 128, 128, 255], [255, 255, 255, 255], 10.0, 0)
                };
                (
                    idx.0,
                    crate::painter::Layer {
                        name: name.0.clone(),
                        opacity: opacity.0,
                        visible: vis.0,
                        blend_mode: blend.0,
                        is_fill,
                        fill_color,
                        fill_noise_color,
                        fill_noise_scale,
                        fill_projection_mode,
                        strokes: strokes.0.clone(),
                    }
                )
            })
            .collect();
        // Sort by index
        layers.sort_by_key(|(idx, _)| *idx);
        layers.into_iter().map(|(_, l)| l).collect()
    }

    pub fn restore_layers_snapshot(&mut self, layers: &[crate::painter::Layer], active_layer_idx: usize, device: &wgpu::Device) {
        let mut world = self.world_mut();

        // 1. Despawn all existing Layer entities
        let old_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<LayerName>>()
            .iter(world)
            .collect();
        for entity in old_entities {
            world.despawn(entity);
        }

        // 2. Spawn new Layer entities
        for (i, layer) in layers.iter().enumerate() {
            let texture = create_layer_texture(device, 1024, 1024);
            let mut entity = world.spawn((
                LayerIndex(i),
                LayerName(layer.name.clone()),
                LayerOpacity(layer.opacity),
                LayerVisibility(layer.visible),
                LayerBlendMode(layer.blend_mode),
                LayerStrokes(layer.strokes.clone()),
                texture,
            ));

            if layer.is_fill {
                entity.insert(FillLayerProperties {
                    color: layer.fill_color,
                    noise_color: layer.fill_noise_color,
                    noise_scale: layer.fill_noise_scale,
                    projection_mode: layer.fill_projection_mode,
                });
            }

            if i == active_layer_idx {
                entity.insert(ActiveLayer);
            }
        }
    }

    pub fn reproject_strokes_ecs(&mut self) {
        let mut world = self.world_mut();

        let mut triangles = Vec::new();
        let mut mesh_query = world.query::<(&Transform, &MeshHandle)>();

        for (transform, mesh_handle) in mesh_query.iter(world) {
            let world_matrix = transform.to_matrix();
            for primitive in &mesh_handle.0.primitives {
                for chunk in primitive.indices.chunks_exact(3) {
                    let i0 = chunk[0] as usize;
                    let i1 = chunk[1] as usize;
                    let i2 = chunk[2] as usize;

                    let v0 = &primitive.vertices[i0];
                    let v1 = &primitive.vertices[i1];
                    let v2 = &primitive.vertices[i2];

                    let p0 = world_matrix.transform_point3(glam::Vec3::from(v0.position));
                    let p1 = world_matrix.transform_point3(glam::Vec3::from(v1.position));
                    let p2 = world_matrix.transform_point3(glam::Vec3::from(v2.position));

                    let uv0 = glam::Vec2::from(v0.tex_coords);
                    let uv1 = glam::Vec2::from(v1.tex_coords);
                    let uv2 = glam::Vec2::from(v2.tex_coords);

                    let center = (p0 + p1 + p2) / 3.0;
                    let bounds_min = p0.min(p1).min(p2);
                    let bounds_max = p0.max(p1).max(p2);

                    triangles.push(crate::painter::WorldTriangle {
                        p0,
                        p1,
                        p2,
                        uv0,
                        uv1,
                        uv2,
                        center,
                        bounds_min,
                        bounds_max,
                    });
                }
            }
        }

        if triangles.is_empty() {
            let mut strokes_query = world.query::<&mut LayerStrokes>();
            for mut strokes in strokes_query.iter_mut(world) {
                for stroke in &mut strokes.0 {
                    stroke.uv_points.clear();
                    for _ in &stroke.points {
                        stroke.uv_points.push(glam::Vec2::ZERO);
                    }
                }
            }
            return;
        }

        let bvh = crate::painter::BVHNode::build(triangles);

        let mut strokes_query = world.query::<&mut LayerStrokes>();
        for mut strokes in strokes_query.iter_mut(world) {
            for stroke in &mut strokes.0 {
                stroke.uv_points.clear();
                for pt in &stroke.points {
                    let mut closest_uv = glam::Vec2::ZERO;
                    let mut min_dist_sq = f32::MAX;
                    bvh.find_closest(*pt, &mut min_dist_sq, &mut closest_uv);
                    stroke.uv_points.push(closest_uv);
                }
            }
        }
    }

    /// Synchronize host-owned app state into ECS domain resource.
    ///
    /// This is the explicit contract while domain mutation is still host-driven.
    /// Call once per frame before ECS schedule execution.
    pub fn sync_domain_state_from(&mut self, app_state: &crate::app::app_state::AppState) {
        if let Some(mut domain) = self.world.get_resource_mut::<DomainStateResource>() {
            domain.0 = app_state.clone();
        } else {
            self.world
                .insert_resource(DomainStateResource(app_state.clone()));
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
        if let Some(mut ops) = self
            .world
            .get_resource_mut::<PendingDomainHostOpsResource>()
        {
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

    /// Retrieve and clear any render error encountered during schedule run.
    pub fn take_render_error(&mut self) -> Option<crate::app::SurfaceError> {
        if let Some(mut err_res) = self.world.get_resource_mut::<RenderErrorResource>() {
            err_res.0.take()
        } else {
            None
        }
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
    use super::*;
    use bevy_ecs::prelude::*;

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

    /// Phase 4.2: Prepare GPU-stage work before render passes (sets flags).
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

    /// Apply GPU prepare changes natively on the GPU.
    pub fn apply_prepare_gpu_system(
        camera: Res<CameraResource>,
        settings: Res<ViewportSettingsResource>,
        document: Option<Res<super::DocumentResource>>,
        camera_gpu: Option<Res<super::CameraGpuResources>>,
        gpu_ctx: Option<Res<super::GpuContextResource>>,
        mut pending_prepare_ops: ResMut<PendingPrepareOpsResource>,
    ) {
        let (Some(gpu), Some(gpu_res), Some(doc)) = (gpu_ctx, camera_gpu, document) else {
            return;
        };
        let ops = std::mem::take(&mut *pending_prepare_ops);
        if ops.update_main_camera_uniform {
            let mut camera_uniform = crate::viewport::CameraUniform::new();
            camera_uniform.update_view_proj(
                &camera,
                settings.light_angle,
                settings.light_intensity,
                settings.ambient_strength,
                settings.view_transform,
                settings.exposure,
                doc.document.num_udim_tiles,
            );
            gpu.queue.write_buffer(
                &gpu_res.buffer,
                0,
                bytemuck::cast_slice(&[camera_uniform]),
            );
        }
    }

    /// Apply pending surface operations directly on the host state.
    pub fn apply_surface_ops_system(
        state_ptr: Res<HostStatePtr>,
        mut pending_ops: ResMut<PendingSurfaceOpsResource>,
        main_ctx: Option<ResMut<super::MainRenderContextResource>>,
    ) {
        let Some(mut main_ctx) = main_ctx else {
            return;
        };
        if state_ptr.0.is_null() {
            return;
        }
        let state = unsafe { &mut *state_ptr.0 };
        let ops = std::mem::take(&mut *pending_ops);

        let main_ctx = &mut *main_ctx;

        state.surface_host.apply_pending_surface_ops(
            ops,
            &mut state.size,
            &mut main_ctx.config,
            &main_ctx.surface,
            &state.device,
            &mut main_ctx.depth_texture,
            &mut main_ctx.depth_view,
            &mut state.viewport,
            &mut state.uv_ui.viewer,
            &mut state.ecs_runtime,
        );
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

    /// Phase 5.1: Begin egui frame lifecycle stage (sets flags).
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

    /// Apply egui begin frame natively on host state.
    pub fn apply_begin_egui_frame_system(
        state_ptr: Res<HostStatePtr>,
        mut pending_ui_ops: ResMut<PendingUiFrameOpsResource>,
    ) {
        if state_ptr.0.is_null() {
            return;
        }
        let state = unsafe { &mut *state_ptr.0 };

        if pending_ui_ops.begin_main_egui_frame {
            pending_ui_ops.begin_main_egui_frame = false;
            let egui_input = state.main_ui.egui_state.take_egui_input(&*state.window);
            state.main_ui.egui_ctx.begin_pass(egui_input);
            state.main_ui.frame_begun = true;
        }
        if pending_ui_ops.begin_uv_egui_frame {
            pending_ui_ops.begin_uv_egui_frame = false;
            if let Some(ref mut viewer) = state.uv_ui.viewer {
                let egui_input = viewer.egui_state.take_egui_input(&*viewer.window);
                viewer.egui_ctx.begin_pass(egui_input);
                state.uv_ui.frame_begun = true;
            }
        }
    }

    /// Phase 5.1: Draw egui panels lifecycle stage (sets flags).
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

    /// Phase 5.1: End egui frame + upload lifecycle stage (sets flags).
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

    /// Native ECS rendering system for the main display surface.
    pub fn ecs_render_main_system(
        state_ptr: Res<HostStatePtr>,
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_ops: ResMut<PendingRenderOpsResource>,
        mut err_res: ResMut<RenderErrorResource>,
        mut failures: bevy_ecs::event::EventWriter<RenderFailureEvent>,
    ) {
        if state_ptr.0.is_null() {
            return;
        }
        let state = unsafe { &mut *state_ptr.0 };

        let mut should_render = false;
        for event in events.read() {
            if matches!(event, RenderRequestEvent::MainSurface) {
                should_render = true;
            }
        }

        let render_scheduler = &state.render_scheduler;
        if should_render || render_scheduler.should_render_main_surface(&pending_ops) {
            pending_ops.render_main_surface = false;
            pending_ops.render_3d_viewport_pass = false;
            pending_ops.render_paint_composite_pass = false;

            let (textures_delta, paint_jobs) = state.draw_main_ui();
            if let Err(err) = state.render_main_surface(textures_delta, paint_jobs) {
                match err {
                    crate::app::SurfaceError::Lost => {
                        failures.send(RenderFailureEvent {
                            surface: RenderSurfaceKind::Main,
                            kind: RenderFailureKind::Lost,
                        });
                    }
                    crate::app::SurfaceError::Outdated => {
                        failures.send(RenderFailureEvent {
                            surface: RenderSurfaceKind::Main,
                            kind: RenderFailureKind::Outdated,
                        });
                    }
                    crate::app::SurfaceError::Timeout => {}
                    crate::app::SurfaceError::Other(_) => {
                        err_res.0 = Some(err);
                    }
                }
            }
        }
    }

    /// Native ECS rendering system for the floatable UV viewer surface.
    pub fn ecs_render_uv_system(
        state_ptr: Res<HostStatePtr>,
        mut events: bevy_ecs::event::EventReader<RenderRequestEvent>,
        mut pending_ops: ResMut<PendingRenderOpsResource>,
        mut err_res: ResMut<RenderErrorResource>,
        mut failures: bevy_ecs::event::EventWriter<RenderFailureEvent>,
    ) {
        if state_ptr.0.is_null() {
            return;
        }
        let state = unsafe { &mut *state_ptr.0 };

        let mut should_render = false;
        for event in events.read() {
            if matches!(event, RenderRequestEvent::UvSurface) {
                should_render = true;
            }
        }

        let render_scheduler = &state.render_scheduler;
        if should_render || render_scheduler.should_render_uv_surface(&pending_ops) {
            pending_ops.render_uv_surface = false;

            if let Some((textures_delta, paint_jobs)) = state.draw_uv_ui() {
                if let Err(err) = state.render_uv_surface(textures_delta, paint_jobs) {
                    match err {
                        crate::app::SurfaceError::Lost => {
                            failures.send(RenderFailureEvent {
                                surface: RenderSurfaceKind::Uv,
                                kind: RenderFailureKind::Lost,
                            });
                        }
                        crate::app::SurfaceError::Outdated => {
                            failures.send(RenderFailureEvent {
                                surface: RenderSurfaceKind::Uv,
                                kind: RenderFailureKind::Outdated,
                            });
                        }
                        crate::app::SurfaceError::Timeout => {}
                        crate::app::SurfaceError::Other(_) => {
                            err_res.0 = Some(err);
                        }
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
                crate::app::ecs::domain::apply_input_state_event_to_app_state(
                    &mut domain.0,
                    command,
                );
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
    pub fn extract_camera_system(
        domain: Res<DomainStateResource>,
        mut extracted_camera: ResMut<ExtractedCameraData>,
    ) {
        let camera = domain.0.camera();
        extracted_camera.eye = camera.eye;
        extracted_camera.target = camera.target;
        extracted_camera.yaw = camera.yaw;
        extracted_camera.pitch = camera.pitch;
        extracted_camera.distance = camera.distance;
        extracted_camera.fov = camera.fov;
        extracted_camera.aspect = camera.aspect;
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
        extracted_layers.layer_visibility = app_state.layer_composition().visibilities.clone();
        extracted_layers.layer_opacities = app_state.layer_composition().opacities.clone();
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

    pub fn camera_update_system(
        mut viewport_events: bevy_ecs::event::EventReader<AppEvent>,
        mut camera: ResMut<CameraResource>,
        mut settings: ResMut<ViewportSettingsResource>,
        gpu_ctx: Option<Res<GpuContextResource>>,
        camera_gpu: Option<Res<CameraGpuResources>>,
        document: Option<Res<DocumentResource>>,
    ) {
        let mut changed = false;
        for event in viewport_events.read() {
            if let AppEvent::Viewport(cmd) = event {
                changed = true;
                match cmd {
                    ViewportCommandEvent::Orbit { dx, dy } => {
                        camera.yaw -= (*dx as f64 * 0.005) as f32;
                        camera.pitch = (camera.pitch + (*dy as f64 * 0.005) as f32).clamp(
                            -std::f32::consts::FRAC_PI_2 + 0.05,
                            std::f32::consts::FRAC_PI_2 - 0.05,
                        );
                    }
                    ViewportCommandEvent::Pan { dx, dy } => {
                        let eye = camera.get_eye();
                        let forward = (camera.target - eye).normalize();
                        let right = forward.cross(glam::Vec3::Y).normalize();
                        let up = right.cross(forward).normalize();
                        let speed = camera.distance * 0.0015;
                        camera.target += right * (-*dx * speed) + up * (*dy * speed);
                    }
                    ViewportCommandEvent::Zoom { scroll } => {
                        camera.distance = (camera.distance - *scroll * 0.25).clamp(1.0, 50.0);
                    }
                }
            }
        }

        if changed {
            if let (Some(gpu), Some(gpu_res), Some(doc)) = (gpu_ctx, camera_gpu, document) {
                let mut camera_uniform = crate::viewport::CameraUniform::new();
                camera_uniform.update_view_proj(
                    &camera,
                    settings.light_angle,
                    settings.light_intensity,
                    settings.ambient_strength,
                    settings.view_transform,
                    settings.exposure,
                    doc.document.num_udim_tiles,
                );
                gpu.queue.write_buffer(
                    &gpu_res.buffer,
                    0,
                    bytemuck::cast_slice(&[camera_uniform]),
                );
            }
        }
    }

    pub fn brush_paint_system(
        mut interaction: ResMut<InteractionStateResource>,
        camera: Res<CameraResource>,
        settings: Res<ViewportSettingsResource>,
        domain: Res<DomainStateResource>,
        prefs: Res<PreferencesResource>,
        registry: Res<SurfaceRegistryResource>,
        gpu_ctx: Option<Res<GpuContextResource>>,
        painter_res: Option<ResMut<PainterResource>>,
        document: Option<Res<DocumentResource>>,
        mesh_query: Query<(&Transform, &MeshHandle, &Aabb)>,
        mut active_layer_query: Query<
            (
                Entity,
                &LayerTexture,
                &mut LayerStrokes,
            ),
            (With<ActiveLayer>, Without<FillLayerProperties>),
        >,
        mut pending_render: ResMut<PendingRenderOpsResource>,
    ) {
        let app_state = &domain.0;
        let is_painting = app_state.input().paint_button_down
            && !app_state.input().orbit_modifier
            && !app_state.input().alt;

        if !is_painting {
            interaction.last_hit_uv = None;
            interaction.last_hit_pos = None;
            return;
        }

        let Some(gpu) = gpu_ctx else { return; };
        let Some(mut painter) = painter_res else { return; };
        let Some(doc) = document else { return; };

        let mouse_pos = glam::Vec2::new(
            app_state.input().last_mouse_pos.x as f32,
            app_state.input().last_mouse_pos.y as f32,
        );
        let screen_size = glam::Vec2::new(
            registry.main_surface_size.0 as f32,
            registry.main_surface_size.1 as f32,
        );

        let eye = camera.get_eye();
        let view = glam::Mat4::look_at_rh(eye, camera.target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(camera.fovy, camera.aspect, camera.znear, camera.zfar);
        let ray = crate::raycast::Ray::from_screen(mouse_pos, screen_size, view, proj);

        let is_eraser = app_state.tool().active_tool == crate::app::types::Tool::Eraser;

        // Apply tablet pressure to brush parameters
        let pressure = if app_state.input().has_tablet_input {
            let min_start = prefs.0.pressure_curve_min_start;
            let max_at = prefs.0.pressure_curve_max_at;
            let p = app_state.input().pen_pressure;
            if p <= min_start {
                0.0
            } else if p >= max_at {
                1.0
            } else {
                (p - min_start) / (max_at - min_start)
            }
        } else {
            1.0
        };
        let effective_size = app_state.canvas().brush_size * (0.2 + 0.8 * pressure);
        let effective_opacity = app_state.canvas().brush_opacity * pressure;
        let mut brush_rgba = app_state.canvas().brush_color;
        brush_rgba[3] = (effective_opacity * 255.0) as u8;

        // Find intersection
        let mut closest_hit: Option<crate::raycast::RaycastHit> = None;
        for (transform, mesh_handle, _aabb) in mesh_query.iter() {
            let world_matrix = transform.to_matrix();
            for primitive in &mesh_handle.0.primitives {
                if let Some(hit) = crate::raycast::intersect_primitive(&ray, primitive, world_matrix) {
                    if let Some(ref current) = closest_hit {
                        if hit.distance < current.distance {
                            closest_hit = Some(hit);
                        }
                    } else {
                        closest_hit = Some(hit);
                    }
                }
            }
        }

        if let Some(hit) = closest_hit {
            if let Ok((_entity, layer_texture, mut strokes)) = active_layer_query.get_single_mut() {
                if interaction.stroke_in_progress.is_none() {
                    interaction.stroke_in_progress = Some(crate::painter::PaintStroke {
                        points: Vec::new(),
                        uv_points: Vec::new(),
                        point_radii: Vec::new(),
                        point_alphas: Vec::new(),
                        color: brush_rgba,
                        radius: effective_size,
                        hardness: app_state.canvas().brush_hardness,
                        is_eraser,
                    });
                }
                if let Some(ref mut stroke) = interaction.stroke_in_progress {
                    stroke.points.push(hit.point);
                    stroke.uv_points.push(hit.uv);
                    stroke.point_radii.push(effective_size);
                    stroke.point_alphas.push(brush_rgba[3]);
                }

                let view_refs: Vec<&wgpu::TextureView> = layer_texture.views.iter().map(|v| &**v).collect();

                if let Some(last_uv) = interaction.last_hit_uv {
                    painter.0.paint_stroke_to_views(
                        &gpu.device,
                        &gpu.queue,
                        &view_refs,
                        last_uv,
                        hit.uv,
                        interaction.last_hit_pos,
                        Some(hit.point),
                        brush_rgba,
                        effective_size,
                        app_state.canvas().brush_hardness,
                        is_eraser,
                        doc.document.num_udim_tiles,
                    );
                } else {
                    painter.0.paint_stamp_to_views(
                        &gpu.device,
                        &gpu.queue,
                        &view_refs,
                        hit.uv,
                        Some(hit.point),
                        brush_rgba,
                        effective_size,
                        app_state.canvas().brush_hardness,
                        is_eraser,
                        doc.document.num_udim_tiles,
                    );
                }

                interaction.last_hit_uv = Some(hit.uv);
                interaction.last_hit_pos = Some(hit.point);
                pending_render.render_main_surface = true;
                pending_render.render_paint_composite_pass = true;
            }
        } else {
            interaction.last_hit_uv = None;
            interaction.last_hit_pos = None;
        }
    }

    pub fn layer_compositor_system(
        gpu_ctx: Option<Res<GpuContextResource>>,
        painter_res: Option<ResMut<PainterResource>>,
        layers_query: Query<(
            &LayerIndex,
            &LayerVisibility,
            &LayerOpacity,
            &LayerBlendMode,
            &LayerTexture,
        )>,
    ) {
        let Some(gpu) = gpu_ctx else { return; };
        let Some(mut painter) = painter_res else { return; };

        // 1. Sort visible layers
        let mut layers: Vec<_> = layers_query.iter().collect();
        layers.sort_by_key(|(idx, _, _, _, _)| idx.0);

        // 2. Copy active layer textures into painter's layer_array_texture slots
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ECS Layer Copy Encoder"),
        });

        let mut active_layers_data = Vec::new();

        for (sorted_idx, (_index, visible, opacity, blend, texture_comp)) in layers.iter().enumerate() {
            if sorted_idx >= crate::painter::MAX_LAYERS {
                break;
            }
            active_layers_data.push((opacity.0, blend.0, visible.0));

            // Copy texture tiles
            for t in 0..crate::painter::MAX_UDIMS {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture_comp.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: t as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &painter.0.layer_array_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: (sorted_idx * crate::painter::MAX_UDIMS + t) as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: painter.0.width,
                        height: painter.0.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));

        // 3. Run composition render pipeline on the GPU
        painter.0.compose_layers_ecs(&gpu.device, &gpu.queue, &active_layers_data);
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
        assert!(runtime
            .world
            .get_resource::<ToolRuntimeResource>()
            .is_some());
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
        assert!(runtime
            .world
            .get_resource::<ExtractedCameraData>()
            .is_some());
        assert!(runtime
            .world
            .get_resource::<ExtractedLayerComposition>()
            .is_some());
        assert!(runtime
            .world
            .get_resource::<ExtractedDocumentData>()
            .is_some());
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
        runtime.send_event(events::AppEvent::Ui(events::UiActionEvent::SetBrushSize(
            123.0,
        )));

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
        runtime.send_event(events::AppEvent::Ui(
            events::UiActionEvent::SetBrushOpacity(0.25),
        ));

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
