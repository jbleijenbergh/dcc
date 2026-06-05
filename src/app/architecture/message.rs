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
