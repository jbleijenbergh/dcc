use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(
        (
            systems::tool_command_system,
            systems::viewport_command_system,
        )
            .in_set(FramePhase::DomainUpdate),
    );
}
