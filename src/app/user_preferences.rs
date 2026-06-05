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
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create settings directory {}: {e}", parent.display()))?;
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
