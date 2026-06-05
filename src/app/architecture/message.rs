use super::input::InputEvent;

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
