use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(systems::window_surface_system.in_set(FramePhase::WinitEventIngest));
    schedule.add_systems(systems::redraw_ingest_system.in_set(FramePhase::WinitEventIngest));
    schedule.add_systems(systems::input_state_system.in_set(FramePhase::DomainUpdate));
}
