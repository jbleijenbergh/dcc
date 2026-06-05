use winit::dpi::PhysicalPosition;

use crate::app::types::Tool;
use crate::painter::Layer;

#[derive(Clone, Debug)]
pub struct AppState {
    pub document: DocumentState,
    pub canvas: CanvasState,
    pub tool: ToolState,
    pub ui: UiState,
    pub history: HistoryState,
    pub resources: ResourceState,
    pub input: InputSnapshot,
}

#[derive(Clone)]
pub struct UndoState {
    pub layers: Vec<Layer>,
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

#[derive(Clone, Debug)]
pub struct DocumentState {
    pub active_layer_idx: usize,
    pub layer_count: usize,
    pub current_mesh: String,
    pub num_udim_tiles: u32,
}

#[derive(Clone, Debug)]
pub struct CanvasState {
    pub brush_size: f32,
    pub brush_color: [u8; 4],
    pub brush_hardness: f32,
    pub brush_opacity: f32,
}

#[derive(Clone, Debug)]
pub struct ToolState {
    pub active_tool: Tool,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub show_uv_viewer: bool,
    pub uv_viewer_source: usize,
    pub uv_viewer_size: f32,
    pub show_uv_wireframe: bool,
    pub show_pressure_calibration: bool,
}

#[derive(Clone, Debug)]
pub struct HistoryState {
    pub undo_len: usize,
    pub redo_len: usize,
    pub undo_stack: Vec<UndoState>,
    pub redo_stack: Vec<UndoState>,
}

#[derive(Clone, Debug)]
pub struct ResourceState {
    pub is_loading_gltf: bool,
    pub has_error: bool,
}

#[derive(Clone, Debug, Default)]
pub struct InputSnapshot {
    pub ctrl: bool,
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
    pub orbit_modifier: bool,
    pub pan_modifier: bool,
    pub paint_button_down: bool,
    pub pan_button_down: bool,
    pub has_tablet_input: bool,
    pub pen_pressure: f32,
    pub touchpad_pressure_stage: i64,
    pub last_mouse_pos: PhysicalPosition<f64>,
}
