use super::input::InputEvent;
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

#[derive(Clone, Debug)]
pub enum Message {
    Input(InputEvent),
    Ui(UiAction),
    Document(DocumentCommand),
}
