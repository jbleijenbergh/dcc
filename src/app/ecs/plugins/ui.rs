use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(systems::ui_action_system.in_set(FramePhase::DomainUpdate));
    schedule.add_systems(
        (
            systems::begin_egui_frame_system,
            systems::draw_egui_panels_system,
        )
            .in_set(FramePhase::RenderMainSurface),
    );
    schedule.add_systems(systems::end_egui_frame_and_upload_system.in_set(FramePhase::EndFrame));
}
