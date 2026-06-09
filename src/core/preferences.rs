use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPreferences {
    pub bindings: InputBindings,
    #[serde(default = "default_pressure_curve_min_start")]
    pub pressure_curve_min_start: f32,
    #[serde(default = "default_pressure_curve_max_at")]
    pub pressure_curve_max_at: f32,
}

fn default_pressure_curve_min_start() -> f32 {
    0.05
}

fn default_pressure_curve_max_at() -> f32 {
    0.85
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            bindings: InputBindings::default(),
            pressure_curve_min_start: 0.05,
            pressure_curve_max_at: 0.85,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub cmd: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub primary_mod: bool,
}

impl KeyBinding {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ctrl: false,
            cmd: false,
            alt: false,
            shift: false,
            primary_mod: false,
        }
    }

    pub fn signature(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.primary_mod {
            parts.push("PrimaryMod");
        }
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.cmd {
            parts.push("Cmd");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MouseBinding {
    pub button: String,
}

impl MouseBinding {
    pub fn new(button: &str) -> Self {
        Self {
            button: button.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputBindings {
    pub orbit_modifier: KeyBinding,
    pub pan_modifier: KeyBinding,
    pub undo: KeyBinding,
    pub redo: KeyBinding,
    pub clear_canvas: KeyBinding,
    pub brush_size_down: KeyBinding,
    pub brush_size_up: KeyBinding,
    pub tool_brush: KeyBinding,
    pub tool_eraser: KeyBinding,
    pub paint_button: MouseBinding,
    pub pan_button: MouseBinding,
}

impl Default for InputBindings {
    fn default() -> Self {
        let mut undo = KeyBinding::new("KeyZ");
        undo.primary_mod = true;

        let mut redo = KeyBinding::new("KeyY");
        redo.primary_mod = true;

        Self {
            orbit_modifier: KeyBinding::new("Space"),
            pan_modifier: KeyBinding::new("AltLeft"),
            undo,
            redo,
            clear_canvas: KeyBinding::new("KeyC"),
            brush_size_down: KeyBinding::new("BracketLeft"),
            brush_size_up: KeyBinding::new("BracketRight"),
            tool_brush: KeyBinding::new("KeyB"),
            tool_eraser: KeyBinding::new("KeyE"),
            paint_button: MouseBinding::new("Left"),
            pan_button: MouseBinding::new("Right"),
        }
    }
}

impl InputBindings {
    pub fn conflicts(&self) -> Vec<String> {
        let key_bindings = [
            ("Orbit Modifier", &self.orbit_modifier),
            ("Pan Modifier", &self.pan_modifier),
            ("Undo", &self.undo),
            ("Redo", &self.redo),
            ("Clear Canvas", &self.clear_canvas),
            ("Brush Size Down", &self.brush_size_down),
            ("Brush Size Up", &self.brush_size_up),
            ("Select Brush Tool", &self.tool_brush),
            ("Select Eraser Tool", &self.tool_eraser),
        ];

        let mouse_bindings = [
            ("Paint Button", &self.paint_button),
            ("Pan Button", &self.pan_button),
        ];

        let mut by_combo: std::collections::BTreeMap<String, Vec<&str>> = std::collections::BTreeMap::new();

        for (name, binding) in key_bindings {
            let key = format!("Key:{}", binding.signature());
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

impl UserPreferences {
    pub fn default_path() -> PathBuf {
        let mut base = if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
        } else {
            PathBuf::from(".")
        };
        base.push(".dcc-painter");
        base.push("settings.toml");
        base
    }

    pub fn load_or_default() -> (Self, PathBuf) {
        let path = Self::default_path();
        if !path.exists() {
            return (Self::default(), path);
        }

        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(parsed) => (parsed, path),
                Err(err) => {
                    log::warn!("Failed to parse settings file {}: {}", path.display(), err);
                    (Self::default(), path)
                }
            },
            Err(err) => {
                log::warn!("Failed to read settings file {}: {}", path.display(), err);
                (Self::default(), path)
            }
        }
    }

    pub fn save_to(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create settings directory {}: {e}",
                    parent.display()
                )
            })?;
        }

        let toml = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings to TOML: {e}"))?;

        fs::write(path, toml)
            .map_err(|e| format!("Failed to write settings file {}: {e}", path.display()))
    }
}

pub const KEY_CHOICES: &[&str] = &[
    "Space",
    "AltLeft",
    "AltRight",
    "ControlLeft",
    "ControlRight",
    "SuperLeft",
    "SuperRight",
    "ShiftLeft",
    "ShiftRight",
    "BracketLeft",
    "BracketRight",
    "KeyA",
    "KeyB",
    "KeyC",
    "KeyD",
    "KeyE",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyI",
    "KeyJ",
    "KeyK",
    "KeyL",
    "KeyM",
    "KeyN",
    "KeyO",
    "KeyP",
    "KeyQ",
    "KeyR",
    "KeyS",
    "KeyT",
    "KeyU",
    "KeyV",
    "KeyW",
    "KeyX",
    "KeyY",
    "KeyZ",
];

pub const MOUSE_BUTTON_CHOICES: &[&str] = &["Left", "Right", "Middle", "Back", "Forward"];
