use crate::app::ecs::events::{
    DocumentCommandEvent, InputStateCommandEvent, ToolCommandEvent, UiActionEvent,
    ViewportCommandEvent,
};
use crate::app::tools::ToolSystem;
use crate::app::{rerender_fill_layer, State, Tool};

pub fn apply_document_event_to_app_state(
    app_state: &mut crate::app::app_state::AppState,
    command: &DocumentCommandEvent,
) {
    match command {
        DocumentCommandEvent::CommitCurrentStroke => {}
        DocumentCommandEvent::ClearAllLayers => {
            app_state.document_mut().active_layer_idx = 0;
            app_state.document_mut().layer_count = 1;
        }
    }
}

pub fn apply_ui_event_to_app_state(
    app_state: &mut crate::app::app_state::AppState,
    ui_action: &UiActionEvent,
) {
    match ui_action {
        UiActionEvent::SelectTool(tool) => {
            app_state.tool_mut().active_tool = match tool {
                crate::app::ecs::events::ToolKind::Brush => Tool::Brush,
                crate::app::ecs::events::ToolKind::Eraser => Tool::Eraser,
            };
        }
        UiActionEvent::AdjustBrushSize(delta) => {
            app_state.canvas_mut().brush_size =
                (app_state.canvas().brush_size + *delta).clamp(2.0, 300.0);
        }
        UiActionEvent::SetBrushSize(size) => {
            app_state.canvas_mut().brush_size = size.clamp(2.0, 300.0);
        }
        UiActionEvent::SetBrushHardness(hardness) => {
            app_state.canvas_mut().brush_hardness = hardness.clamp(0.0, 1.0);
        }
        UiActionEvent::SetBrushOpacity(opacity) => {
            app_state.canvas_mut().brush_opacity = opacity.clamp(0.0, 1.0);
        }
        UiActionEvent::SetBrushColor(color) => {
            app_state.canvas_mut().brush_color = *color;
        }
        UiActionEvent::SetUvViewerVisible(visible) => {
            app_state.ui_mut().show_uv_viewer = *visible;
        }
        UiActionEvent::SetUvViewerSource(source) => {
            app_state.ui_mut().uv_viewer_source = *source;
        }
        UiActionEvent::SetUvViewerSize(size) => {
            app_state.ui_mut().uv_viewer_size = size.clamp(64.0, 512.0);
        }
        UiActionEvent::SetUvWireframe(show) => {
            app_state.ui_mut().show_uv_wireframe = *show;
        }
        UiActionEvent::SetCurrentMesh(mesh) => {
            app_state.document_mut().current_mesh = mesh.clone();
        }
        UiActionEvent::SetPressureCurve { .. } => {}
        UiActionEvent::StartGltfLoad => {
            app_state.resources_mut().is_loading_gltf = true;
            app_state.resources_mut().has_error = false;
        }
        UiActionEvent::FinishGltfLoadSuccess { filename } => {
            app_state.resources_mut().is_loading_gltf = false;
            app_state.resources_mut().has_error = false;
            app_state.document_mut().current_mesh = filename.clone();
        }
        UiActionEvent::FinishGltfLoadError { .. } => {
            app_state.resources_mut().is_loading_gltf = false;
            app_state.resources_mut().has_error = true;
        }
        UiActionEvent::DismissLoadError => {
            app_state.resources_mut().has_error = false;
        }
        UiActionEvent::SetActiveScene(_)
        | UiActionEvent::SetImportSeams(_)
        | UiActionEvent::SetImportMargin(_)
        | UiActionEvent::SetImportOrientation(_)
        | UiActionEvent::SwitchMesh(_)
        | UiActionEvent::RecomputeUvsAndReproject
        | UiActionEvent::SelectLayer(_)
        | UiActionEvent::AddPaintLayer(_)
        | UiActionEvent::AddUvGridLayer
        | UiActionEvent::AddUvCheckerLayer
        | UiActionEvent::AddFillLayer
        | UiActionEvent::DeleteLayer(_)
        | UiActionEvent::SetLayerVisible { .. }
        | UiActionEvent::SetLayerBlendMode { .. }
        | UiActionEvent::SetLayerOpacity { .. }
        | UiActionEvent::SetFillBaseColor { .. }
        | UiActionEvent::SetFillNoiseColor { .. }
        | UiActionEvent::SetFillNoiseScale { .. }
        | UiActionEvent::SetFillProjectionMode { .. }
        | UiActionEvent::ClearCanvas
        | UiActionEvent::Undo
        | UiActionEvent::Redo => {}
    }
}

pub fn apply_viewport_event(state: &mut State, viewport_cmd: &ViewportCommandEvent) {
    match viewport_cmd {
        ViewportCommandEvent::Orbit { dx, dy } => {
            state.viewport.camera.yaw -= (*dx as f64 * 0.005) as f32;
            state.viewport.camera.pitch =
                (state.viewport.camera.pitch + (*dy as f64 * 0.005) as f32).clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.05,
                    std::f32::consts::FRAC_PI_2 - 0.05,
                );
            state.interaction.last_hit_uv = None;
            state.interaction.last_hit_pos = None;
        }
        ViewportCommandEvent::Pan { dx, dy } => {
            let eye = state.viewport.camera.get_eye();
            let forward = (state.viewport.camera.target - eye).normalize();
            let right = forward.cross(glam::Vec3::Y).normalize();
            let up = right.cross(forward).normalize();

            let speed = state.viewport.camera.distance * 0.0015;
            state.viewport.camera.target += right * (-*dx * speed) + up * (*dy * speed);
        }
        ViewportCommandEvent::Zoom { scroll } => {
            state.viewport.camera.distance =
                (state.viewport.camera.distance - *scroll * 0.25).clamp(1.0, 50.0);
        }
    }
}

pub fn apply_input_state_event(state: &mut State, input_cmd: &InputStateCommandEvent) {
    apply_input_state_event_to_app_state(&mut state.app_state, input_cmd);
}

pub fn apply_input_state_event_to_app_state(
    app_state: &mut crate::app::app_state::AppState,
    input_cmd: &InputStateCommandEvent,
) {
    match input_cmd {
        InputStateCommandEvent::UpdateModifier { key, is_pressed } => {
            use winit::keyboard::KeyCode;
            match key {
                KeyCode::ControlLeft | KeyCode::ControlRight => {
                    app_state.input_mut().ctrl = *is_pressed
                }
                KeyCode::SuperLeft | KeyCode::SuperRight => app_state.input_mut().cmd = *is_pressed,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    app_state.input_mut().shift = *is_pressed
                }
                KeyCode::AltLeft | KeyCode::AltRight => app_state.input_mut().alt = *is_pressed,
                _ => {}
            }
        }
        InputStateCommandEvent::UpdateModifiersSnapshot(modifiers) => {
            app_state.input_mut().ctrl = modifiers.ctrl;
            app_state.input_mut().cmd = modifiers.cmd;
            app_state.input_mut().shift = modifiers.shift;
            app_state.input_mut().alt = modifiers.alt;
        }
        InputStateCommandEvent::UpdateMousePosition(pointer) => {
            app_state.input_mut().last_mouse_pos = winit::dpi::PhysicalPosition::new(
                pointer.screen_position.x as f64,
                pointer.screen_position.y as f64,
            );
            if let Some(pressure) = pointer.pressure {
                app_state.input_mut().has_tablet_input = true;
                app_state.input_mut().pen_pressure = pressure.clamp(0.0, 1.0);
            }
        }
        InputStateCommandEvent::SetPaintButtonDown(down) => {
            app_state.input_mut().paint_button_down = *down;
        }
        InputStateCommandEvent::SetPanButtonDown(down) => {
            app_state.input_mut().pan_button_down = *down;
        }
        InputStateCommandEvent::SetOrbitModifier(active) => {
            app_state.input_mut().orbit_modifier = *active;
        }
        InputStateCommandEvent::SetAltModifier(active) => {
            app_state.input_mut().alt = *active;
        }
        InputStateCommandEvent::ResetPenPressure => {
            app_state.input_mut().pen_pressure = 1.0;
            if app_state.input().touchpad_pressure_stage <= 0 {
                app_state.input_mut().has_tablet_input = false;
            }
        }
    }
}

pub fn apply_document_event(state: &mut State, command: &DocumentCommandEvent) {
    apply_document_event_to_app_state(&mut state.app_state, command);

    match command {
        DocumentCommandEvent::CommitCurrentStroke => state.commit_current_stroke(),
        DocumentCommandEvent::ClearAllLayers => {
            state.push_undo_state();
            state.painter.clear_all_layers(&state.device, &state.queue);
        }
    }
}

pub fn apply_tool_event(
    state: &mut State,
    tool_cmd: &ToolCommandEvent,
    tool_system: &mut ToolSystem,
) {
    match tool_cmd {
        ToolCommandEvent::PointerDown(pointer) => {
            tool_system.with_active_handler(state, |tool, s| {
                tool.on_pointer_down(s, pointer);
            });
        }
        ToolCommandEvent::PointerMove(pointer) => {
            tool_system.with_active_handler(state, |tool, s| {
                tool.on_pointer_move(s, pointer);
            });
        }
        ToolCommandEvent::PointerUp(pointer) => {
            state.interaction.last_hit_uv = None;
            state.interaction.last_hit_pos = None;
            tool_system.with_active_handler(state, |tool, s| {
                tool.on_pointer_up(s, pointer);
            });
        }
        ToolCommandEvent::PointerCancel(pointer) => {
            state.interaction.last_hit_uv = None;
            state.interaction.last_hit_pos = None;
            tool_system.with_active_handler(state, |tool, s| {
                tool.on_pointer_cancel(s, pointer);
            });
        }
    }
}

pub fn apply_ui_event(state: &mut State, ui_action: &UiActionEvent) {
    apply_ui_event_to_app_state(&mut state.app_state, ui_action);

    match ui_action {
        UiActionEvent::SelectTool(_)
        | UiActionEvent::AdjustBrushSize(_)
        | UiActionEvent::SetBrushSize(_)
        | UiActionEvent::SetBrushHardness(_)
        | UiActionEvent::SetBrushOpacity(_)
        | UiActionEvent::SetBrushColor(_)
        | UiActionEvent::SetUvViewerVisible(_)
        | UiActionEvent::SetUvViewerSource(_)
        | UiActionEvent::SetUvViewerSize(_)
        | UiActionEvent::SetUvWireframe(_)
        | UiActionEvent::SetCurrentMesh(_)
        | UiActionEvent::StartGltfLoad => {}
        UiActionEvent::SwitchMesh(mesh) => {
            state.push_undo_state();
            state.toggle_mesh(mesh);
            state.app_state.document_mut().current_mesh = mesh.clone();
        }
        UiActionEvent::SetActiveScene(scene_idx) => {
            if *scene_idx < state.viewport.document.scenes.len() {
                state.viewport.document.active_scene_idx = *scene_idx;
            }
        }
        UiActionEvent::SetImportSeams(seams) => {
            state.import_settings.seams_option = *seams;
        }
        UiActionEvent::SetImportMargin(margin) => {
            state.import_settings.margin_size = *margin;
        }
        UiActionEvent::SetImportOrientation(orientation) => {
            state.import_settings.island_orientation = *orientation;
        }
        UiActionEvent::RecomputeUvsAndReproject => {
            state.push_undo_state();
            state.recompute_and_reproject();
        }
        UiActionEvent::SetPressureCurve { min_start, max_at } => {
            let clamped_min = min_start.clamp(0.0, 1.0);
            let clamped_max = max_at.clamp(clamped_min + 0.001, 1.0);
            state.preferences.pressure_curve_min_start = clamped_min;
            state.preferences.pressure_curve_max_at = clamped_max;
        }
        UiActionEvent::FinishGltfLoadSuccess { .. } => {
            state.ui_state.error_details = None;
            state.ui_state.error_time = None;
        }
        UiActionEvent::FinishGltfLoadError { path, message } => {
            state.ui_state.error_details = Some(crate::app::LoadError {
                path: path.clone(),
                message: message.clone(),
            });
            state.ui_state.error_time = Some(std::time::Instant::now());
        }
        UiActionEvent::DismissLoadError => {
            state.ui_state.error_details = None;
            state.ui_state.error_time = None;
        }
        UiActionEvent::SelectLayer(idx) => {
            if *idx < state.painter.layers.len() {
                state.painter.active_layer_idx = *idx;
            }
        }
        UiActionEvent::AddPaintLayer(name) => {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                state.push_undo_state();
                state
                    .painter
                    .add_paint_layer(trimmed.to_string(), &state.device, &state.queue);
            }
        }
        UiActionEvent::AddUvGridLayer => {
            state.push_undo_state();
            state
                .painter
                .load_uv_grid_layer(&state.device, &state.queue);
        }
        UiActionEvent::AddUvCheckerLayer => {
            state.push_undo_state();
            state
                .painter
                .load_uv_checker_layer(&state.device, &state.queue);
        }
        UiActionEvent::AddFillLayer => {
            state.push_undo_state();
            let name = format!("Fill {}", state.painter.layers.len() + 1);
            state.painter.add_fill_layer(
                name,
                &state.device,
                &state.queue,
                &state.viewport.document,
            );
        }
        UiActionEvent::DeleteLayer(idx) => {
            if state.painter.layers.len() > 1 && *idx < state.painter.layers.len() {
                state.push_undo_state();
                state
                    .painter
                    .delete_layer(*idx, &state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerVisible { idx, visible } => {
            if *idx < state.painter.layers.len() && state.painter.layers[*idx].visible != *visible {
                state.push_undo_state();
                state.painter.layers[*idx].visible = *visible;
                state.painter.compose_layers(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerBlendMode { idx, mode } => {
            if *idx < state.painter.layers.len() && state.painter.layers[*idx].blend_mode != *mode {
                state.push_undo_state();
                state.painter.layers[*idx].blend_mode = *mode;
                state.painter.compose_layers(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerOpacity {
            idx,
            opacity,
            begin_undo,
        } => {
            if *idx < state.painter.layers.len() {
                if *begin_undo {
                    state.push_undo_state();
                }
                state.painter.layers[*idx].opacity = opacity.clamp(0.0, 1.0);
                state.painter.compose_layers(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetFillBaseColor {
            idx,
            color,
            begin_undo,
        } => {
            if *idx < state.painter.layers.len() && state.painter.layers[*idx].is_fill {
                if *begin_undo {
                    state.push_undo_state();
                }
                state.painter.layers[*idx].fill_color = *color;
                rerender_fill_layer(state, *idx);
            }
        }
        UiActionEvent::SetFillNoiseColor {
            idx,
            color,
            begin_undo,
        } => {
            if *idx < state.painter.layers.len() && state.painter.layers[*idx].is_fill {
                if *begin_undo {
                    state.push_undo_state();
                }
                state.painter.layers[*idx].fill_noise_color = *color;
                rerender_fill_layer(state, *idx);
            }
        }
        UiActionEvent::SetFillNoiseScale {
            idx,
            scale,
            begin_undo,
        } => {
            if *idx < state.painter.layers.len() && state.painter.layers[*idx].is_fill {
                if *begin_undo {
                    state.push_undo_state();
                }
                state.painter.layers[*idx].fill_noise_scale = *scale;
                rerender_fill_layer(state, *idx);
            }
        }
        UiActionEvent::SetFillProjectionMode { idx, mode } => {
            if *idx < state.painter.layers.len()
                && state.painter.layers[*idx].is_fill
                && state.painter.layers[*idx].fill_projection_mode != *mode
            {
                state.push_undo_state();
                state.painter.layers[*idx].fill_projection_mode = *mode;
                rerender_fill_layer(state, *idx);
            }
        }
        UiActionEvent::ClearCanvas => {
            state.push_undo_state();
            state.painter.clear_all_layers(&state.device, &state.queue);
        }
        UiActionEvent::Undo => state.undo(),
        UiActionEvent::Redo => state.redo(),
    }
}
