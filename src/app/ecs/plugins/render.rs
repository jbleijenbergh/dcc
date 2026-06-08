use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(systems::apply_surface_ops_system.in_set(FramePhase::InputResolve));
    schedule.add_systems(
        (
            systems::extract_camera_system,
            systems::extract_layer_composition_system,
        )
            .in_set(FramePhase::ExtractRenderData),
    );
    schedule.add_systems(
        (
            systems::prepare_gpu_system,
            systems::apply_prepare_gpu_system,
        )
            .chain()
            .in_set(FramePhase::PrepareGpu),
    );
    schedule.add_systems(
        (
            systems::render_3d_viewport_system,
            systems::render_paint_composite_system,
            systems::layer_compositor_system,
            systems::ecs_render_main_system,
        )
            .chain()
            .in_set(FramePhase::RenderMainSurface)
            .after(systems::draw_egui_panels_system),
    );
    schedule.add_systems(
        (
            systems::render_aux_surface_system,
            systems::ecs_render_uv_system,
        )
            .chain()
            .in_set(FramePhase::RenderAuxSurfaces),
    );
    schedule.add_systems(systems::render_recovery_system.in_set(FramePhase::EndFrame));
}

