use super::input::{ModifiersSnapshot, PointerData};
use crate::painter::BlendMode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolKind {
    Brush,
    Eraser,
}

#[derive(Clone, Debug)]
pub enum UiAction {
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

#[derive(Clone, Debug)]
pub enum DocumentCommand {
    CommitCurrentStroke,
    ClearAllLayers,
}

#[derive(Clone, Copy, Debug)]
pub enum ViewportCommand {
    Orbit { dx: f32, dy: f32 },
    Pan { dx: f32, dy: f32 },
    Zoom { scroll: f32 },
}

#[derive(Clone, Copy, Debug)]
pub enum ToolCommand {
    PointerDown(PointerData),
    PointerMove(PointerData),
    PointerUp(PointerData),
    PointerCancel(PointerData),
}

#[derive(Clone, Copy, Debug)]
pub enum InputStateCommand {
    UpdateModifier { key: winit::keyboard::KeyCode, is_pressed: bool },
    UpdateModifiersSnapshot(ModifiersSnapshot),
    UpdateMousePosition(PointerData),
    SetPaintButtonDown(bool),
    SetPanButtonDown(bool),
    SetOrbitModifier(bool),
    SetAltModifier(bool),
    ResetPenPressure,
}

#[derive(Clone, Debug)]
pub enum Message {
    Ui(UiAction),
    Document(DocumentCommand),
    Viewport(ViewportCommand),
    Tool(ToolCommand),
    InputState(InputStateCommand),
}

