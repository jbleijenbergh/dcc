use bevy_ecs::prelude::*;

use crate::app::ecs::{systems, FramePhase};

pub fn register(schedule: &mut Schedule) {
    schedule.add_systems(systems::window_surface_system.in_set(FramePhase::WinitEventIngest));
    schedule.add_systems(systems::redraw_ingest_system.in_set(FramePhase::WinitEventIngest));
    schedule.add_systems(systems::input_state_system.in_set(FramePhase::DomainUpdate));
}

impl crate::app::State {
    pub fn calibrated_pressure(&self) -> f32 {
        let p = self.app_state.input().pen_pressure.clamp(0.0, 1.0);
        let min_start = self.preferences.pressure_curve_min_start.clamp(0.0, 1.0);
        let max_at = self
            .preferences
            .pressure_curve_max_at
            .clamp(min_start + 0.001, 1.0);
        ((p - min_start) / (max_at - min_start)).clamp(0.0, 1.0)
    }

    pub fn input(&mut self, event: &winit::event::WindowEvent) -> bool {
        let egui_resp = self
            .main_ui
            .egui_state
            .on_window_event(&*self.window, event);
        if egui_resp.consumed {
            return true;
        }
        let events = crate::core::input::normalize_window_event(
            self.app_state.input(),
            &self.preferences.bindings,
            event,
        );
        let mut consumed = false;
        for event in events {
            self.emit_event(event);
            consumed = true;
        }

        self.sync_app_state_snapshot();
        consumed
    }
}
