use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(systems::document_command_system.in_set(FramePhase::DomainUpdate));
    schedule.add_systems(systems::extract_document_data_system.in_set(FramePhase::ExtractRenderData));
}
