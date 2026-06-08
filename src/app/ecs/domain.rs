use crate::app::ecs::events::{
    DocumentCommandEvent, InputStateCommandEvent, ToolCommandEvent, UiActionEvent,
    ViewportCommandEvent,
};
use crate::app::tools::ToolSystem;
use crate::app::{rerender_fill_layer, State, Tool};
use crate::app::ecs;
use bevy_ecs::prelude::*;

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
    let mut world = state.ecs_runtime.world_mut();
    if let Some(mut camera) = world.get_resource_mut::<crate::app::ecs::CameraResource>() {
        match viewport_cmd {
            ViewportCommandEvent::Orbit { dx, dy } => {
                camera.yaw -= (*dx as f64 * 0.005) as f32;
                camera.pitch = (camera.pitch + (*dy as f64 * 0.005) as f32).clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.05,
                    std::f32::consts::FRAC_PI_2 - 0.05,
                );
                state.interaction.last_hit_uv = None;
                state.interaction.last_hit_pos = None;
            }
            ViewportCommandEvent::Pan { dx, dy } => {
                let eye = camera.get_eye();
                let forward = (camera.target - eye).normalize();
                let right = forward.cross(glam::Vec3::Y).normalize();
                let up = right.cross(forward).normalize();

                let speed = camera.distance * 0.0015;
                camera.target += right * (-*dx * speed) + up * (*dy * speed);
            }
            ViewportCommandEvent::Zoom { scroll } => {
                camera.distance = (camera.distance - *scroll * 0.25).clamp(1.0, 50.0);
            }
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
            state.ecs_runtime.clear_all_layers_ecs(&state.device, &state.queue);
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
            if let Some(mut doc) = state.ecs_runtime.world_mut().get_resource_mut::<crate::app::ecs::DocumentResource>() {
                if *scene_idx < doc.document.scenes.len() {
                    doc.document.active_scene_idx = *scene_idx;
                }
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
            let mut target_entity = None;
            let mut active_entities = Vec::new();
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex)>();
                for (entity, layer_idx) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        target_entity = Some(entity);
                    }
                }
                let mut active_query = world.query_filtered::<Entity, With<ecs::ActiveLayer>>();
                for entity in active_query.iter(world) {
                    active_entities.push(entity);
                }
                for entity in active_entities {
                    world.entity_mut(entity).remove::<ecs::ActiveLayer>();
                }
                if let Some(entity) = target_entity {
                    world.entity_mut(entity).insert(ecs::ActiveLayer);
                }
            }
        }
        UiActionEvent::AddPaintLayer(name) => {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                state.push_undo_state();
                let texture = ecs::create_layer_texture(&state.device, 1024, 1024);
                let mut world = state.ecs_runtime.world_mut();
                let next_idx = {
                    let mut query = world.query::<&ecs::LayerIndex>();
                    query.iter(world).map(|idx| idx.0).max().map(|max| max + 1).unwrap_or(0)
                };
                let mut active_entities = Vec::new();
                let mut active_query = world.query_filtered::<Entity, With<ecs::ActiveLayer>>();
                for entity in active_query.iter(world) {
                    active_entities.push(entity);
                }
                for entity in active_entities {
                    world.entity_mut(entity).remove::<ecs::ActiveLayer>();
                }
                world.spawn((
                    ecs::LayerName(trimmed.to_string()),
                    ecs::LayerOpacity(1.0),
                    ecs::LayerVisibility(true),
                    ecs::LayerBlendMode(crate::painter::BlendMode::Normal),
                    ecs::LayerIndex(next_idx),
                    texture,
                    ecs::LayerStrokes(Vec::new()),
                    ecs::ActiveLayer,
                ));
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::AddUvGridLayer => {
            state.push_undo_state();
            state.ecs_runtime.load_uv_grid_layer_ecs(&state.device, &state.queue);
        }
        UiActionEvent::AddUvCheckerLayer => {
            state.push_undo_state();
            state.ecs_runtime.load_uv_checker_layer_ecs(&state.device, &state.queue);
        }
        UiActionEvent::AddFillLayer => {
            state.push_undo_state();
            let layer_count = {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<&ecs::LayerName>();
                query.iter(world).count()
            };
            let name = format!("Fill {}", layer_count + 1);
            state.ecs_runtime.add_fill_layer_ecs(name, &state.device, &state.queue);
        }
        UiActionEvent::DeleteLayer(idx) => {
            let layer_count = {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<&ecs::LayerName>();
                query.iter(world).count()
            };
            if layer_count > 1 {
                state.push_undo_state();
                state.ecs_runtime.delete_layer_ecs(*idx, &state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerVisible { idx, visible } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::LayerVisibility)>();
                for (entity, layer_idx, layer_vis) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        if layer_vis.0 != *visible {
                            target_entity = Some(entity);
                        }
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                state.push_undo_state();
                let mut world = state.ecs_runtime.world_mut();
                world.entity_mut(entity).insert(ecs::LayerVisibility(*visible));
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerBlendMode { idx, mode } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::LayerBlendMode)>();
                for (entity, layer_idx, layer_blend) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        if layer_blend.0 != *mode {
                            target_entity = Some(entity);
                        }
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                state.push_undo_state();
                let mut world = state.ecs_runtime.world_mut();
                world.entity_mut(entity).insert(ecs::LayerBlendMode(*mode));
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetLayerOpacity {
            idx,
            opacity,
            begin_undo,
        } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex)>();
                for (entity, layer_idx) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        target_entity = Some(entity);
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                if *begin_undo {
                    state.push_undo_state();
                }
                let mut world = state.ecs_runtime.world_mut();
                world.entity_mut(entity).insert(ecs::LayerOpacity(opacity.clamp(0.0, 1.0)));
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetFillBaseColor {
            idx,
            color,
            begin_undo,
        } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::FillLayerProperties)>();
                for (entity, layer_idx, _) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        target_entity = Some(entity);
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                if *begin_undo {
                    state.push_undo_state();
                }
                let mut world = state.ecs_runtime.world_mut();
                if let Some(mut fill) = world.entity_mut(entity).get_mut::<ecs::FillLayerProperties>() {
                    fill.color = *color;
                }
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetFillNoiseColor {
            idx,
            color,
            begin_undo,
        } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::FillLayerProperties)>();
                for (entity, layer_idx, _) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        target_entity = Some(entity);
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                if *begin_undo {
                    state.push_undo_state();
                }
                let mut world = state.ecs_runtime.world_mut();
                if let Some(mut fill) = world.entity_mut(entity).get_mut::<ecs::FillLayerProperties>() {
                    fill.noise_color = *color;
                }
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetFillNoiseScale {
            idx,
            scale,
            begin_undo,
        } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::FillLayerProperties)>();
                for (entity, layer_idx, _) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        target_entity = Some(entity);
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                if *begin_undo {
                    state.push_undo_state();
                }
                let mut world = state.ecs_runtime.world_mut();
                if let Some(mut fill) = world.entity_mut(entity).get_mut::<ecs::FillLayerProperties>() {
                    fill.noise_scale = *scale;
                }
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::SetFillProjectionMode { idx, mode } => {
            let mut target_entity = None;
            {
                let mut world = state.ecs_runtime.world_mut();
                let mut query = world.query::<(Entity, &ecs::LayerIndex, &ecs::FillLayerProperties)>();
                for (entity, layer_idx, fill) in query.iter(world) {
                    if layer_idx.0 == *idx {
                        if fill.projection_mode != *mode {
                            target_entity = Some(entity);
                        }
                        break;
                    }
                }
            }
            if let Some(entity) = target_entity {
                state.push_undo_state();
                let mut world = state.ecs_runtime.world_mut();
                if let Some(mut fill) = world.entity_mut(entity).get_mut::<ecs::FillLayerProperties>() {
                    fill.projection_mode = *mode;
                }
                state.ecs_runtime.redraw_all_layers_ecs(&state.device, &state.queue);
            }
        }
        UiActionEvent::ClearCanvas => {
            state.push_undo_state();
            state.ecs_runtime.clear_all_layers_ecs(&state.device, &state.queue);
        }
        UiActionEvent::Undo => state.undo(),
        UiActionEvent::Redo => state.redo(),
    }
}
