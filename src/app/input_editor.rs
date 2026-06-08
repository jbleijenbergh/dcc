use super::input;
use super::user_preferences::{KEY_CHOICES, MOUSE_BUTTON_CHOICES};
use super::State;
use std::collections::BTreeMap;
use winit::event::WindowEvent;

impl State {
    pub fn calibrated_pressure(&self) -> f32 {
        let p = self.app_state.input().pen_pressure.clamp(0.0, 1.0);
        let min_start = self.preferences.pressure_curve_min_start.clamp(0.0, 1.0);
        let max_at = self
            .preferences
            .pressure_curve_max_at
            .clamp(min_start + 0.001, 1.0);
        ((p - min_start) / (max_at - min_start)).clamp(0.0, 1.0)
    }

    pub(crate) fn save_settings(&mut self) {
        log::debug!("Saving bindings to {}", self.preferences_path.display());
        match self.preferences.save_to(&self.preferences_path) {
            Ok(()) => {
                let feedback = format!("Saved settings to {}", self.preferences_path.display());
                log::debug!(
                    "Bindings saved successfully: orbit_mod={}, pan_mod={}, undo={}, redo={}",
                    self.preferences.bindings.orbit_modifier.key,
                    self.preferences.bindings.pan_modifier.key,
                    self.preferences.bindings.undo.key,
                    self.preferences.bindings.redo.key
                );
                self.ui_state.settings_feedback = Some(feedback);
            }
            Err(e) => {
                self.ui_state.settings_feedback = Some(format!("Failed to save settings: {}", e));
                log::error!(
                    "Failed to save bindings to {}: {}",
                    self.preferences_path.display(),
                    e
                );
            }
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        let egui_resp = self
            .main_ui
            .egui_state
            .on_window_event(&*self.window, event);
        if egui_resp.consumed {
            return true;
        }
        let events = input::normalize_window_event(
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

    pub(crate) fn draw_key_binding_editor(
        ui: &mut egui::Ui,
        id: &str,
        binding: &mut super::user_preferences::KeyBinding,
    ) {
        ui.horizontal(|ui| {
            ui.label("Key");
            egui::ComboBox::from_id_salt(id)
                .selected_text(&binding.key)
                .show_ui(ui, |ui| {
                    for key in KEY_CHOICES {
                        ui.selectable_value(&mut binding.key, (*key).to_string(), *key);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut binding.primary_mod, "Primary Mod (Ctrl/Cmd)");
            ui.checkbox(&mut binding.ctrl, "Ctrl");
            ui.checkbox(&mut binding.cmd, "Cmd");
            ui.checkbox(&mut binding.alt, "Alt");
            ui.checkbox(&mut binding.shift, "Shift");
        });
    }

    pub(crate) fn draw_mouse_binding_editor(
        ui: &mut egui::Ui,
        id: &str,
        binding: &mut super::user_preferences::MouseBinding,
    ) {
        ui.horizontal(|ui| {
            ui.label("Button");
            egui::ComboBox::from_id_salt(id)
                .selected_text(&binding.button)
                .show_ui(ui, |ui| {
                    for button in MOUSE_BUTTON_CHOICES {
                        ui.selectable_value(&mut binding.button, (*button).to_string(), *button);
                    }
                });
        });
    }

    pub(crate) fn key_binding_signature(binding: &super::user_preferences::KeyBinding) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if binding.primary_mod {
            parts.push("PrimaryMod");
        }
        if binding.ctrl {
            parts.push("Ctrl");
        }
        if binding.cmd {
            parts.push("Cmd");
        }
        if binding.alt {
            parts.push("Alt");
        }
        if binding.shift {
            parts.push("Shift");
        }
        parts.push(&binding.key);
        parts.join("+")
    }

    pub(crate) fn binding_conflicts(
        bindings: &super::user_preferences::InputBindings,
    ) -> Vec<String> {
        let key_bindings = [
            ("Orbit Modifier", &bindings.orbit_modifier),
            ("Pan Modifier", &bindings.pan_modifier),
            ("Undo", &bindings.undo),
            ("Redo", &bindings.redo),
            ("Clear Canvas", &bindings.clear_canvas),
            ("Brush Size Down", &bindings.brush_size_down),
            ("Brush Size Up", &bindings.brush_size_up),
            ("Select Brush Tool", &bindings.tool_brush),
            ("Select Eraser Tool", &bindings.tool_eraser),
        ];

        let mouse_bindings = [
            ("Paint Button", &bindings.paint_button),
            ("Pan Button", &bindings.pan_button),
        ];

        let mut by_combo: BTreeMap<String, Vec<&str>> = BTreeMap::new();

        for (name, binding) in key_bindings {
            let key = format!("Key:{}", Self::key_binding_signature(binding));
            by_combo.entry(key).or_default().push(name);
        }

        for (name, binding) in mouse_bindings {
            let key = format!("Mouse:{}", binding.button);
            by_combo.entry(key).or_default().push(name);
        }

        let mut warnings = Vec::new();
        for (combo, actions) in by_combo {
            if actions.len() > 1 {
                warnings.push(format!(
                    "{} is assigned to multiple actions: {}",
                    combo,
                    actions.join(", ")
                ));
            }
        }

        warnings
    }
}
