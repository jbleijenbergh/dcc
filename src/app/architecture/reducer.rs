use std::sync::OnceLock;

use crate::app::input::{ModifiersSnapshot, PointerData};
use super::message::{DocumentCommand, Message, ToolKind, UiAction, ViewportCommand, ToolCommand, InputStateCommand};
use super::tool::ToolSystem;
use super::super::rerender_fill_layer;
use crate::app::types::Tool;
use crate::app::State;

fn tool_system() -> &'static std::sync::Mutex<ToolSystem> {
    static TOOL_SYSTEM: OnceLock<std::sync::Mutex<ToolSystem>> = OnceLock::new();
    TOOL_SYSTEM.get_or_init(|| std::sync::Mutex::new(ToolSystem::default()))
}

fn update_modifier_state(state: &mut State, key: winit::keyboard::KeyCode, is_pressed: bool) {
    use winit::keyboard::KeyCode;
    match key {
        KeyCode::ControlLeft | KeyCode::ControlRight => state.app_state.input_mut().ctrl = is_pressed,
        KeyCode::SuperLeft | KeyCode::SuperRight => state.app_state.input_mut().cmd = is_pressed,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.app_state.input_mut().shift = is_pressed,
        KeyCode::AltLeft | KeyCode::AltRight => state.app_state.input_mut().alt = is_pressed,
        _ => {}
    }
}

fn update_mouse_position(state: &mut State, pointer: &PointerData) {
    state.app_state.input_mut().last_mouse_pos = winit::dpi::PhysicalPosition::new(
        pointer.screen_position.x as f64,
        pointer.screen_position.y as f64,
    );
}

fn apply_pointer_pressure(state: &mut State, pointer: &PointerData) {
    if let Some(pressure) = pointer.pressure {
        state.app_state.input_mut().has_tablet_input = true;
        state.app_state.input_mut().pen_pressure = pressure.clamp(0.0, 1.0);
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

fn apply_modifiers_snapshot(state: &mut State, modifiers: ModifiersSnapshot) {
    state.app_state.input_mut().ctrl = modifiers.ctrl;
    state.app_state.input_mut().cmd = modifiers.cmd;
    state.app_state.input_mut().shift = modifiers.shift;
    state.app_state.input_mut().alt = modifiers.alt;
}

pub fn dispatch(state: &mut State, message: Message) -> bool {
    match message {
        Message::Viewport(viewport_cmd) => {
            match viewport_cmd {
                ViewportCommand::Orbit { dx, dy } => {
                    state.viewport.camera.yaw -= (dx as f64 * 0.005) as f32;
                    state.viewport.camera.pitch =
                        (state.viewport.camera.pitch + (dy as f64 * 0.005) as f32).clamp(
                            -std::f32::consts::FRAC_PI_2 + 0.05,
                            std::f32::consts::FRAC_PI_2 - 0.05,
                        );
                    state.interaction.last_hit_uv = None;
                    state.interaction.last_hit_pos = None;
                }
                ViewportCommand::Pan { dx, dy } => {
                    let eye = state.viewport.camera.get_eye();
                    let forward = (state.viewport.camera.target - eye).normalize();
                    let right = forward.cross(glam::Vec3::Y).normalize();
                    let up = right.cross(forward).normalize();

                    let speed = state.viewport.camera.distance * 0.0015;
                    state.viewport.camera.target +=
                        right * (-dx * speed) + up * (dy * speed);
                }
                ViewportCommand::Zoom { scroll } => {
                    state.viewport.camera.distance =
                        (state.viewport.camera.distance - scroll * 0.25).max(1.0).min(50.0);
                }
            }
            true
        }
        Message::Tool(tool_cmd) => {
            match tool_cmd {
                ToolCommand::PointerDown(pointer) => {
                    apply_tool_pointer_down(state, &pointer);
                }
                ToolCommand::PointerMove(pointer) => {
                    apply_tool_pointer_move(state, &pointer);
                }
                ToolCommand::PointerUp(pointer) => {
                    state.interaction.last_hit_uv = None;
                    state.interaction.last_hit_pos = None;
                    apply_tool_pointer_up(state, &pointer);
                }
                ToolCommand::PointerCancel(pointer) => {
                    state.interaction.last_hit_uv = None;
                    state.interaction.last_hit_pos = None;
                    apply_tool_pointer_cancel(state, &pointer);
                }
            }
            true
        }
        Message::InputState(input_state_cmd) => {
            match input_state_cmd {
                InputStateCommand::UpdateModifier { key, is_pressed } => {
                    update_modifier_state(state, key, is_pressed);
                }
                InputStateCommand::UpdateModifiersSnapshot(modifiers) => {
                    apply_modifiers_snapshot(state, modifiers);
                }
                InputStateCommand::UpdateMousePosition(pointer) => {
                    update_mouse_position(state, &pointer);
                    apply_pointer_pressure(state, &pointer);
                }
                InputStateCommand::SetPaintButtonDown(down) => {
                    state.app_state.input_mut().paint_button_down = down;
                }
                InputStateCommand::SetPanButtonDown(down) => {
                    state.app_state.input_mut().pan_button_down = down;
                }
                InputStateCommand::SetOrbitModifier(active) => {
                    state.app_state.input_mut().orbit_modifier = active;
                }
                InputStateCommand::SetAltModifier(active) => {
                    state.app_state.input_mut().alt = active;
                }
                InputStateCommand::ResetPenPressure => {
                    state.app_state.input_mut().pen_pressure = 1.0;
                    if state.app_state.input().touchpad_pressure_stage <= 0 {
                        state.app_state.input_mut().has_tablet_input = false;
                    }
                }
            }
            true
        }
        Message::Ui(action) => {
            match action {
                UiAction::SelectTool(tool) => {
                    state.app_state.tool_mut().active_tool = match tool {
                        ToolKind::Brush => Tool::Brush,
                        ToolKind::Eraser => Tool::Eraser,
                    };
                }
                UiAction::AdjustBrushSize(delta) => {
                    state.app_state.canvas_mut().brush_size = (state.app_state.canvas().brush_size + delta).clamp(2.0, 300.0);
                }
                UiAction::SetBrushSize(size) => {
                    state.app_state.canvas_mut().brush_size = size.clamp(2.0, 300.0);
                }
                UiAction::SetBrushHardness(hardness) => {
                    state.app_state.canvas_mut().brush_hardness = hardness.clamp(0.0, 1.0);
                }
                UiAction::SetBrushOpacity(opacity) => {
                    state.app_state.canvas_mut().brush_opacity = opacity.clamp(0.0, 1.0);
                }
                UiAction::SetBrushColor(color) => {
                    state.app_state.canvas_mut().brush_color = color;
                }
                UiAction::SetUvViewerVisible(visible) => {
                    state.app_state.ui_mut().show_uv_viewer = visible;
                }
                UiAction::SetUvViewerSource(source) => {
                    state.app_state.ui_mut().uv_viewer_source = source;
                }
                UiAction::SetUvViewerSize(size) => {
                    state.app_state.ui_mut().uv_viewer_size = size.clamp(64.0, 512.0);
                }
                UiAction::SetUvWireframe(show) => {
                    state.app_state.ui_mut().show_uv_wireframe = show;
                }
                UiAction::SwitchMesh(mesh) => {
                    state.push_undo_state();
                    state.toggle_mesh(&mesh);
                    state.app_state.document_mut().current_mesh = mesh;
                }
                UiAction::SetCurrentMesh(mesh) => {
                    state.app_state.document_mut().current_mesh = mesh;
                }
                UiAction::SetActiveScene(scene_idx) => {
                    if scene_idx < state.viewport.document.scenes.len() {
                        state.viewport.document.active_scene_idx = scene_idx;
                    }
                }
                UiAction::SetImportSeams(seams) => {
                    state.import_settings.seams_option = seams;
                }
                UiAction::SetImportMargin(margin) => {
                    state.import_settings.margin_size = margin;
                }
                UiAction::SetImportOrientation(orientation) => {
                    state.import_settings.island_orientation = orientation;
                }
                UiAction::RecomputeUvsAndReproject => {
                    state.push_undo_state();
                    state.recompute_and_reproject();
                }
                UiAction::SetPressureCurve { min_start, max_at } => {
                    let clamped_min = min_start.clamp(0.0, 1.0);
                    let clamped_max = max_at.clamp(clamped_min + 0.001, 1.0);
                    state.preferences.pressure_curve_min_start = clamped_min;
                    state.preferences.pressure_curve_max_at = clamped_max;
                }
                UiAction::StartGltfLoad => {
                    state.app_state.resources_mut().is_loading_gltf = true;
                    state.app_state.resources_mut().has_error = false;
                }
                UiAction::FinishGltfLoadSuccess { filename } => {
                    state.app_state.resources_mut().is_loading_gltf = false;
                    state.app_state.resources_mut().has_error = false;
                    state.app_state.document_mut().current_mesh = filename;
                    state.ui_state.error_details = None;
                    state.ui_state.error_time = None;
                }
                UiAction::FinishGltfLoadError { path, message } => {
                    state.app_state.resources_mut().is_loading_gltf = false;
                    state.app_state.resources_mut().has_error = true;
                    state.ui_state.error_details = Some(crate::app::LoadError { path, message });
                    state.ui_state.error_time = Some(std::time::Instant::now());
                }
                UiAction::DismissLoadError => {
                    state.ui_state.error_details = None;
                    state.ui_state.error_time = None;
                    state.app_state.resources_mut().has_error = false;
                }
                UiAction::SelectLayer(idx) => {
                    if idx < state.painter.layers.len() {
                        state.painter.active_layer_idx = idx;
                    }
                }
                UiAction::AddPaintLayer(name) => {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        state.push_undo_state();
                        state.painter.add_paint_layer(trimmed.to_string(), &state.device, &state.queue);
                    }
                }
                UiAction::AddUvGridLayer => {
                    state.push_undo_state();
                    state.painter.load_uv_grid_layer(&state.device, &state.queue);
                }
                UiAction::AddUvCheckerLayer => {
                    state.push_undo_state();
                    state.painter.load_uv_checker_layer(&state.device, &state.queue);
                }
                UiAction::AddFillLayer => {
                    state.push_undo_state();
                    let name = format!("Fill {}", state.painter.layers.len() + 1);
                    state.painter.add_fill_layer(name, &state.device, &state.queue, &state.viewport.document);
                }
                UiAction::DeleteLayer(idx) => {
                    if state.painter.layers.len() > 1 && idx < state.painter.layers.len() {
                        state.push_undo_state();
                        state.painter.delete_layer(idx, &state.device, &state.queue);
                    }
                }
                UiAction::SetLayerVisible { idx, visible } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].visible != visible {
                        state.push_undo_state();
                        state.painter.layers[idx].visible = visible;
                        state.painter.compose_layers(&state.device, &state.queue);
                    }
                }
                UiAction::SetLayerBlendMode { idx, mode } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].blend_mode != mode {
                        state.push_undo_state();
                        state.painter.layers[idx].blend_mode = mode;
                        state.painter.compose_layers(&state.device, &state.queue);
                    }
                }
                UiAction::SetLayerOpacity { idx, opacity, begin_undo } => {
                    if idx < state.painter.layers.len() {
                        if begin_undo {
                            state.push_undo_state();
                        }
                        state.painter.layers[idx].opacity = opacity.clamp(0.0, 1.0);
                        state.painter.compose_layers(&state.device, &state.queue);
                    }
                }
                UiAction::SetFillBaseColor { idx, color, begin_undo } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].is_fill {
                        if begin_undo {
                            state.push_undo_state();
                        }
                        state.painter.layers[idx].fill_color = color;
                        rerender_fill_layer(state, idx);
                    }
                }
                UiAction::SetFillNoiseColor { idx, color, begin_undo } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].is_fill {
                        if begin_undo {
                            state.push_undo_state();
                        }
                        state.painter.layers[idx].fill_noise_color = color;
                        rerender_fill_layer(state, idx);
                    }
                }
                UiAction::SetFillNoiseScale { idx, scale, begin_undo } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].is_fill {
                        if begin_undo {
                            state.push_undo_state();
                        }
                        state.painter.layers[idx].fill_noise_scale = scale;
                        rerender_fill_layer(state, idx);
                    }
                }
                UiAction::SetFillProjectionMode { idx, mode } => {
                    if idx < state.painter.layers.len() && state.painter.layers[idx].is_fill && state.painter.layers[idx].fill_projection_mode != mode {
                        state.push_undo_state();
                        state.painter.layers[idx].fill_projection_mode = mode;
                        rerender_fill_layer(state, idx);
                    }
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
