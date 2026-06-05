use std::sync::OnceLock;

use super::input::{InputEvent, ModifiersSnapshot, PointerData};
use super::message::{DocumentCommand, Message, ToolKind, UiAction};
use super::tool::ToolSystem;
use crate::app::types::Tool;
use crate::app::State;

fn tool_system() -> &'static std::sync::Mutex<ToolSystem> {
    static TOOL_SYSTEM: OnceLock<std::sync::Mutex<ToolSystem>> = OnceLock::new();
    TOOL_SYSTEM.get_or_init(|| std::sync::Mutex::new(ToolSystem::default()))
}

fn update_modifier_state(state: &mut State, key: winit::keyboard::KeyCode, is_pressed: bool) {
    use winit::keyboard::KeyCode;
    match key {
        KeyCode::ControlLeft | KeyCode::ControlRight => state.app_state.input.ctrl = is_pressed,
        KeyCode::SuperLeft | KeyCode::SuperRight => state.app_state.input.cmd = is_pressed,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.app_state.input.shift = is_pressed,
        KeyCode::AltLeft | KeyCode::AltRight => state.app_state.input.alt = is_pressed,
        _ => {}
    }
}

fn update_mouse_position(state: &mut State, pointer: &PointerData) {
    state.app_state.input.last_mouse_pos = winit::dpi::PhysicalPosition::new(
        pointer.screen_position.x as f64,
        pointer.screen_position.y as f64,
    );
}

fn apply_pointer_pressure(state: &mut State, pointer: &PointerData) {
    if let Some(pressure) = pointer.pressure {
        state.app_state.input.has_tablet_input = true;
        state.app_state.input.pen_pressure = pressure.clamp(0.0, 1.0);
    }
}

fn apply_tool_pointer_down(state: &mut State, pointer: &PointerData) {
    if let Ok(mut tools) = tool_system().lock() {
        tools.with_active_handler(state, |tool, s| {
            tool.on_pointer_down(s, pointer);
        });
    }
}

fn apply_tool_pointer_move(state: &mut State, pointer: &PointerData) {
    if let Ok(mut tools) = tool_system().lock() {
        tools.with_active_handler(state, |tool, s| {
            tool.on_pointer_move(s, pointer);
        });
    }
}

fn apply_tool_pointer_up(state: &mut State, pointer: &PointerData) {
    if let Ok(mut tools) = tool_system().lock() {
        tools.with_active_handler(state, |tool, s| {
            tool.on_pointer_up(s, pointer);
        });
    }
}

fn apply_tool_pointer_cancel(state: &mut State, pointer: &PointerData) {
    if let Ok(mut tools) = tool_system().lock() {
        tools.with_active_handler(state, |tool, s| {
            tool.on_pointer_cancel(s, pointer);
        });
    }
}

fn apply_key_action(state: &mut State, key: winit::keyboard::KeyCode, pressed: bool) -> bool {
    if !pressed {
        return false;
    }

    if state.binding_matches_key(&state.preferences.bindings.undo, key) {
        state.undo();
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.redo, key) {
        state.redo();
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.brush_size_down, key) {
        state.app_state.canvas.brush_size = (state.app_state.canvas.brush_size - 5.0).max(2.0);
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.brush_size_up, key) {
        state.app_state.canvas.brush_size = (state.app_state.canvas.brush_size + 5.0).min(300.0);
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.clear_canvas, key) {
        state.push_undo_state();
        state.painter.clear_all_layers(&state.device, &state.queue);
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.tool_brush, key) {
        state.app_state.tool.active_tool = Tool::Brush;
        return true;
    }
    if state.binding_matches_key(&state.preferences.bindings.tool_eraser, key) {
        state.app_state.tool.active_tool = Tool::Eraser;
        return true;
    }

    false
}

fn apply_modifiers_snapshot(state: &mut State, modifiers: ModifiersSnapshot) {
    state.app_state.input.ctrl = modifiers.ctrl;
    state.app_state.input.cmd = modifiers.cmd;
    state.app_state.input.shift = modifiers.shift;
    state.app_state.input.alt = modifiers.alt;
}

pub fn dispatch(state: &mut State, message: Message) -> bool {
    match message {
        Message::Input(input) => match input {
            InputEvent::PointerDown(pointer) => {
                update_mouse_position(state, &pointer);
                apply_pointer_pressure(state, &pointer);

                if state.binding_matches_mouse(
                    &state.preferences.bindings.paint_button,
                    winit::event::MouseButton::Left,
                ) {
                    state.app_state.input.paint_button_down = true;
                    if !state.app_state.input.orbit_modifier && !state.app_state.input.alt {
                        apply_tool_pointer_down(state, &pointer);
                    }
                    return true;
                }
                false
            }
            InputEvent::PointerMove(pointer) => {
                let dx = pointer.delta.x as f64;
                let dy = pointer.delta.y as f64;
                update_mouse_position(state, &pointer);
                apply_pointer_pressure(state, &pointer);

                if state.app_state.input.paint_button_down {
                    if state.app_state.input.orbit_modifier || state.app_state.input.alt {
                        state.viewport.camera.yaw -= (dx * 0.005) as f32;
                        state.viewport.camera.pitch =
                            (state.viewport.camera.pitch + (dy * 0.005) as f32).clamp(
                                -std::f32::consts::FRAC_PI_2 + 0.05,
                                std::f32::consts::FRAC_PI_2 - 0.05,
                            );
                        state.app_state.input.last_hit_uv = None;
                        state.app_state.input.last_hit_pos = None;
                    } else {
                        apply_tool_pointer_move(state, &pointer);
                    }
                    return true;
                }

                if state.app_state.input.pan_button_down {
                    let eye = state.viewport.camera.get_eye();
                    let forward = (state.viewport.camera.target - eye).normalize();
                    let right = forward.cross(glam::Vec3::Y).normalize();
                    let up = right.cross(forward).normalize();

                    let speed = state.viewport.camera.distance * 0.0015;
                    state.viewport.camera.target +=
                        right * (-dx as f32 * speed) + up * (dy as f32 * speed);
                    return true;
                }

                false
            }
            InputEvent::PointerUp(pointer) => {
                update_mouse_position(state, &pointer);
                state.app_state.input.paint_button_down = false;
                state.app_state.input.last_hit_uv = None;
                state.app_state.input.last_hit_pos = None;
                state.app_state.input.pen_pressure = 1.0;
                if state.app_state.input.touchpad_pressure_stage <= 0 {
                    state.app_state.input.has_tablet_input = false;
                }
                apply_tool_pointer_up(state, &pointer);
                true
            }
            InputEvent::PointerCancel(pointer) => {
                update_mouse_position(state, &pointer);
                state.app_state.input.paint_button_down = false;
                state.app_state.input.last_hit_uv = None;
                state.app_state.input.last_hit_pos = None;
                apply_tool_pointer_cancel(state, &pointer);
                true
            }
            InputEvent::PointerEnter(pointer) => {
                update_mouse_position(state, &pointer);
                true
            }
            InputEvent::PointerLeave(pointer) => {
                update_mouse_position(state, &pointer);
                true
            }
            InputEvent::Wheel { delta, .. } => {
                let scroll = delta.y;
                state.viewport.camera.distance =
                    (state.viewport.camera.distance - scroll * 0.25).max(1.0).min(50.0);
                true
            }
            InputEvent::KeyDown { key, .. } => {
                update_modifier_state(state, key, true);

                if state.binding_matches_key(&state.preferences.bindings.orbit_modifier, key) {
                    state.app_state.input.orbit_modifier = true;
                    return true;
                }
                if state.binding_matches_key(&state.preferences.bindings.pan_modifier, key) {
                    state.app_state.input.alt = true;
                    return true;
                }

                apply_key_action(state, key, true)
            }
            InputEvent::KeyUp { key, .. } => {
                update_modifier_state(state, key, false);

                if state.binding_matches_key(&state.preferences.bindings.orbit_modifier, key) {
                    state.app_state.input.orbit_modifier = false;
                    return true;
                }
                if state.binding_matches_key(&state.preferences.bindings.pan_modifier, key) {
                    state.app_state.input.alt = false;
                    return true;
                }

                false
            }
            InputEvent::ModifiersChanged(modifiers) => {
                apply_modifiers_snapshot(state, modifiers);
                true
            }
        },
        Message::Ui(action) => {
            match action {
                UiAction::SelectTool(tool) => {
                    state.app_state.tool.active_tool = match tool {
                        ToolKind::Brush => Tool::Brush,
                        ToolKind::Eraser => Tool::Eraser,
                    };
                }
                UiAction::AdjustBrushSize(delta) => {
                    state.app_state.canvas.brush_size = (state.app_state.canvas.brush_size + delta).clamp(2.0, 300.0);
                }
                UiAction::SetBrushSize(size) => {
                    state.app_state.canvas.brush_size = size.clamp(2.0, 300.0);
                }
                UiAction::ClearCanvas => {
                    state.push_undo_state();
                    state.painter.clear_all_layers(&state.device, &state.queue);
                }
                UiAction::Undo => state.undo(),
                UiAction::Redo => state.redo(),
            }
            true
        }
        Message::Document(command) => {
            match command {
                DocumentCommand::CommitCurrentStroke => state.commit_current_stroke(),
                DocumentCommand::ClearAllLayers => {
                    state.push_undo_state();
                    state.painter.clear_all_layers(&state.device, &state.queue);
                }
            }
            true
        }
    }
}
