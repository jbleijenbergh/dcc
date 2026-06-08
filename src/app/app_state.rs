use getset::Getters;
use winit::dpi::PhysicalPosition;

use crate::app::types::Tool;
use crate::painter::Layer;

#[derive(Clone, Debug, Getters)]
pub struct AppState {
    #[getset(get = "pub")]
    document: DocumentState,
    #[getset(get = "pub")]
    canvas: CanvasState,
    #[getset(get = "pub")]
    tool: ToolState,
    #[getset(get = "pub")]
    ui: UiState,
    #[getset(get = "pub")]
    history: HistoryState,
    #[getset(get = "pub")]
    resources: ResourceState,
    #[getset(get = "pub")]
    input: InputSnapshot,
    #[getset(get = "pub")]
    camera: CameraState,
    #[getset(get = "pub")]
    layer_composition: LayerCompositionState,
}

impl AppState {
    /// Construct a new AppState with default values.
    pub fn new() -> Self {
        Self {
            document: DocumentState {
                active_layer_idx: 0,
                layer_count: 1,
                current_mesh: String::new(),
                num_udim_tiles: 1,
            },
            canvas: CanvasState {
                brush_size: 50.0,
                brush_color: [0, 0, 0, 255],
                brush_hardness: 1.0,
                brush_opacity: 1.0,
            },
            tool: ToolState {
                active_tool: Tool::Brush,
            },
            ui: UiState {
                show_uv_viewer: false,
                uv_viewer_source: 0,
                uv_viewer_size: 256.0,
                show_uv_wireframe: false,
                show_pressure_calibration: false,
            },
            history: HistoryState {
                undo_len: 0,
                redo_len: 0,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
            },
            resources: ResourceState {
                is_loading_gltf: false,
                has_error: false,
            },
            input: InputSnapshot::default(),
            camera: CameraState::default(),
            layer_composition: LayerCompositionState::default(),
        }
    }

    pub(crate) fn document_mut(&mut self) -> &mut DocumentState {
        &mut self.document
    }
    pub(crate) fn canvas_mut(&mut self) -> &mut CanvasState {
        &mut self.canvas
    }
    pub(crate) fn tool_mut(&mut self) -> &mut ToolState {
        &mut self.tool
    }
    pub(crate) fn ui_mut(&mut self) -> &mut UiState {
        &mut self.ui
    }
    pub(crate) fn history_mut(&mut self) -> &mut HistoryState {
        &mut self.history
    }
    pub(crate) fn resources_mut(&mut self) -> &mut ResourceState {
        &mut self.resources
    }
    pub(crate) fn input_mut(&mut self) -> &mut InputSnapshot {
        &mut self.input
    }
    pub(crate) fn camera_mut(&mut self) -> &mut CameraState {
        &mut self.camera
    }
    pub(crate) fn layer_composition_mut(&mut self) -> &mut LayerCompositionState {
        &mut self.layer_composition
    }
}

#[derive(Clone, Getters)]
pub struct UndoState {
    #[getset(get, get_mut)]
    pub layers: Vec<Layer>,
    #[getset(get, get_mut)]
    pub active_layer_idx: usize,
}

impl std::fmt::Debug for UndoState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoState")
            .field("layer_count", &self.layers.len())
            .field("active_layer_idx", &self.active_layer_idx)
            .finish()
    }
}

#[derive(Clone, Debug, Getters)]
pub struct DocumentState {
    #[getset(get, get_mut)]
    pub active_layer_idx: usize,
    #[getset(get, get_mut)]
    pub layer_count: usize,
    #[getset(get, get_mut)]
    pub current_mesh: String,
    #[getset(get, get_mut)]
    pub num_udim_tiles: u32,
}

#[derive(Clone, Debug, Getters)]
pub struct CanvasState {
    #[getset(get, get_mut)]
    pub brush_size: f32,
    #[getset(get, get_mut)]
    pub brush_color: [u8; 4],
    #[getset(get, get_mut)]
    pub brush_hardness: f32,
    #[getset(get, get_mut)]
    pub brush_opacity: f32,
}

#[derive(Clone, Debug, Getters)]
pub struct ToolState {
    #[getset(get, get_mut)]
    pub active_tool: Tool,
}

#[derive(Clone, Debug, Getters)]
pub struct UiState {
    #[getset(get, get_mut)]
    pub show_uv_viewer: bool,
    #[getset(get, get_mut)]
    pub uv_viewer_source: usize,
    #[getset(get, get_mut)]
    pub uv_viewer_size: f32,
    #[getset(get, get_mut)]
    pub show_uv_wireframe: bool,
    #[getset(get, get_mut)]
    pub show_pressure_calibration: bool,
}

#[derive(Clone, Debug, Getters)]
pub struct HistoryState {
    #[getset(get, get_mut)]
    pub undo_len: usize,
    #[getset(get, get_mut)]
    pub redo_len: usize,
    #[getset(get, get_mut)]
    pub undo_stack: Vec<UndoState>,
    #[getset(get, get_mut)]
    pub redo_stack: Vec<UndoState>,
}

#[derive(Clone, Debug, Getters)]
pub struct ResourceState {
    #[getset(get, get_mut)]
    pub is_loading_gltf: bool,
    #[getset(get, get_mut)]
    pub has_error: bool,
}

#[derive(Clone, Debug, Default, Getters)]
pub struct InputSnapshot {
    #[getset(get, get_mut)]
    pub ctrl: bool,
    #[getset(get, get_mut)]
    pub cmd: bool,
    #[getset(get, get_mut)]
    pub shift: bool,
    #[getset(get, get_mut)]
    pub alt: bool,
    #[getset(get, get_mut)]
    pub orbit_modifier: bool,
    #[getset(get, get_mut)]
    pub pan_modifier: bool,
    #[getset(get, get_mut)]
    pub paint_button_down: bool,
    #[getset(get, get_mut)]
    pub pan_button_down: bool,
    #[getset(get, get_mut)]
    pub has_tablet_input: bool,
    #[getset(get, get_mut)]
    pub pen_pressure: f32,
    #[getset(get, get_mut)]
    pub touchpad_pressure_stage: i64,
    #[getset(get, get_mut)]
    pub last_mouse_pos: PhysicalPosition<f64>,
}

#[derive(Clone, Debug, Getters, Default)]
pub struct CameraState {
    #[getset(get, get_mut)]
    pub eye: glam::Vec3,
    #[getset(get, get_mut)]
    pub target: glam::Vec3,
    #[getset(get, get_mut)]
    pub yaw: f32,
    #[getset(get, get_mut)]
    pub pitch: f32,
    #[getset(get, get_mut)]
    pub distance: f32,
    #[getset(get, get_mut)]
    pub fov: f32,
    #[getset(get, get_mut)]
    pub aspect: f32,
}

#[derive(Clone, Debug, Getters, Default)]
pub struct LayerCompositionState {
    #[getset(get, get_mut)]
    pub visibilities: Vec<bool>,
    #[getset(get, get_mut)]
    pub opacities: Vec<f32>,
}
