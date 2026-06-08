use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(
        (
            systems::tool_command_system,
            systems::viewport_command_system,
            systems::camera_update_system,
            systems::brush_paint_system,
        )
            .in_set(FramePhase::DomainUpdate),
    );
}
