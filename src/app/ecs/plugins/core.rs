use bevy_ecs::prelude::*;

use crate::app::ecs::FramePhase;

pub fn register(schedule: &mut Schedule) {
    schedule.configure_sets(
        (
            FramePhase::WinitEventIngest,
            FramePhase::InputResolve,
            FramePhase::DomainUpdate,
            FramePhase::ExtractRenderData,
            FramePhase::PrepareGpu,
            FramePhase::RenderMainSurface,
            FramePhase::RenderAuxSurfaces,
            FramePhase::EndFrame,
        )
            .chain(),
    );
}
