use super::input::PointerData;
use super::message::ToolKind;
use crate::app::types::Tool;
use crate::app::State;

pub trait ToolHandler {
    fn kind(&self) -> ToolKind;
    fn on_pointer_down(&mut self, state: &mut State, pointer: &PointerData);
    fn on_pointer_move(&mut self, state: &mut State, pointer: &PointerData);
    fn on_pointer_up(&mut self, state: &mut State, pointer: &PointerData);
    fn on_pointer_cancel(&mut self, state: &mut State, pointer: &PointerData);
}

#[derive(Default)]
pub struct BrushTool;

#[derive(Default)]
pub struct EraserTool;

impl ToolHandler for BrushTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Brush
    }

    fn on_pointer_down(&mut self, state: &mut State, _pointer: &PointerData) {
        state.app_state.tool_mut().active_tool = Tool::Brush;
        if !state.app_state.input().orbit_modifier && !state.app_state.input().alt {
            state.paint_at_cursor();
        }
    }

    fn on_pointer_move(&mut self, state: &mut State, _pointer: &PointerData) {
        state.app_state.tool_mut().active_tool = Tool::Brush;
        if state.app_state.input().paint_button_down && !state.app_state.input().orbit_modifier && !state.app_state.input().alt {
            state.paint_at_cursor();
        }
    }

    fn on_pointer_up(&mut self, _state: &mut State, _pointer: &PointerData) {}

    fn on_pointer_cancel(&mut self, state: &mut State, _pointer: &PointerData) {
        state.interaction.last_hit_uv = None;
        state.interaction.last_hit_pos = None;
    }
}

impl ToolHandler for EraserTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Eraser
    }

    fn on_pointer_down(&mut self, state: &mut State, _pointer: &PointerData) {
        state.app_state.tool_mut().active_tool = Tool::Eraser;
        if !state.app_state.input().orbit_modifier && !state.app_state.input().alt {
            state.paint_at_cursor();
        }
    }

    fn on_pointer_move(&mut self, state: &mut State, _pointer: &PointerData) {
        state.app_state.tool_mut().active_tool = Tool::Eraser;
        if state.app_state.input().paint_button_down && !state.app_state.input().orbit_modifier && !state.app_state.input().alt {
            state.paint_at_cursor();
        }
    }

    fn on_pointer_up(&mut self, _state: &mut State, _pointer: &PointerData) {}

    fn on_pointer_cancel(&mut self, state: &mut State, _pointer: &PointerData) {
        state.interaction.last_hit_uv = None;
        state.interaction.last_hit_pos = None;
    }
}

pub struct ToolSystem {
    brush: BrushTool,
    eraser: EraserTool,
}

impl Default for ToolSystem {
    fn default() -> Self {
        Self {
            brush: BrushTool,
            eraser: EraserTool,
        }
    }
}

impl ToolSystem {
    pub fn with_active_handler<R>(
        &mut self,
        state: &mut State,
        mut call: impl FnMut(&mut dyn ToolHandler, &mut State) -> R,
    ) -> R {
        match state.app_state.tool().active_tool {
            Tool::Brush => call(&mut self.brush, state),
            Tool::Eraser => call(&mut self.eraser, state),
        }
    }
}
