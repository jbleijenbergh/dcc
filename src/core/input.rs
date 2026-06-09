use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::app::app_state::InputSnapshot;
use crate::ecs::events::{
    AppEvent, DocumentCommandEvent, InputStateCommandEvent, ToolCommandEvent, ToolKind,
    UiActionEvent, ViewportCommandEvent,
};

pub type PointerId = u64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PointerDeviceKind {
    Mouse,
    Pen,
    Touch,
    Trackpad,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HoverState {
    Hovering,
    Contact,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ModifiersSnapshot {
    pub ctrl: bool,
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PointerButtonSnapshot {
    pub primary: bool,
    pub secondary: bool,
    pub middle: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PointerData {
    pub id: PointerId,
    pub device_kind: PointerDeviceKind,
    pub screen_position: glam::Vec2,
    pub canvas_position: Option<glam::Vec2>,
    pub world_position: Option<glam::Vec3>,
    pub delta: glam::Vec2,
    pub pressure: Option<f32>,
    pub tilt: Option<glam::Vec2>,
    pub barrel_button: Option<bool>,
    pub buttons: PointerButtonSnapshot,
    pub modifiers: ModifiersSnapshot,
    pub hover_state: HoverState,
    pub timestamp: std::time::Instant,
}

fn parse_key_code(key: &str) -> Option<KeyCode> {
    match key {
        "Space" => Some(KeyCode::Space),
        "AltLeft" => Some(KeyCode::AltLeft),
        "AltRight" => Some(KeyCode::AltRight),
        "ControlLeft" => Some(KeyCode::ControlLeft),
        "ControlRight" => Some(KeyCode::ControlRight),
        "SuperLeft" => Some(KeyCode::SuperLeft),
        "SuperRight" => Some(KeyCode::SuperRight),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ShiftRight" => Some(KeyCode::ShiftRight),
        "BracketLeft" => Some(KeyCode::BracketLeft),
        "BracketRight" => Some(KeyCode::BracketRight),
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        _ => None,
    }
}

fn parse_mouse_button(button: &str) -> Option<MouseButton> {
    match button {
        "Left" => Some(MouseButton::Left),
        "Right" => Some(MouseButton::Right),
        "Middle" => Some(MouseButton::Middle),
        "Back" => Some(MouseButton::Back),
        "Forward" => Some(MouseButton::Forward),
        _ => None,
    }
}

fn binding_matches_key(
    input: &InputSnapshot,
    binding: &crate::core::preferences::KeyBinding,
    key: KeyCode,
) -> bool {
    let Some(expected) = parse_key_code(&binding.key) else {
        return false;
    };

    if expected != key {
        return false;
    }

    if binding.primary_mod {
        if !(input.ctrl || input.cmd) {
            return false;
        }
    }

    if binding.ctrl && !input.ctrl {
        return false;
    }
    if binding.cmd && !input.cmd {
        return false;
    }
    if binding.alt && !input.alt {
        return false;
    }
    if binding.shift && !input.shift {
        return false;
    }

    true
}

fn binding_matches_mouse(
    binding: &crate::core::preferences::MouseBinding,
    button: MouseButton,
) -> bool {
    parse_mouse_button(&binding.button).map_or(false, |expected| expected == button)
}

fn pointer_from_position(
    input: &InputSnapshot,
    pos: PhysicalPosition<f64>,
    delta: glam::Vec2,
    device_kind: PointerDeviceKind,
    pressure: Option<f32>,
    hover_state: HoverState,
) -> PointerData {
    PointerData {
        id: 0,
        device_kind,
        screen_position: glam::Vec2::new(pos.x as f32, pos.y as f32),
        canvas_position: None,
        world_position: None,
        delta,
        pressure,
        tilt: None,
        barrel_button: None,
        buttons: PointerButtonSnapshot {
            primary: input.paint_button_down,
            secondary: input.pan_button_down,
            middle: false,
        },
        modifiers: ModifiersSnapshot {
            ctrl: input.ctrl,
            cmd: input.cmd,
            shift: input.shift,
            alt: input.alt,
        },
        hover_state,
        timestamp: std::time::Instant::now(),
    }
}

pub fn normalize_window_event(
    input: &InputSnapshot,
    bindings: &crate::core::preferences::InputBindings,
    event: &WindowEvent,
) -> Vec<AppEvent> {
    let mut out = Vec::new();

    match event {
        WindowEvent::TouchpadPressure {
            pressure, stage, ..
        } => {
            let mut modifiers = ModifiersSnapshot {
                ctrl: input.ctrl,
                cmd: input.cmd,
                shift: input.shift,
                alt: input.alt,
            };
            if *stage <= 0 {
                modifiers.alt = input.alt;
            }
            out.push(AppEvent::InputState(
                InputStateCommandEvent::UpdateModifiersSnapshot(modifiers),
            ));

            let pressure_pointer = pointer_from_position(
                input,
                input.last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Trackpad,
                Some(pressure.clamp(0.0, 1.0) as f32),
                if input.paint_button_down {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );

            if *stage > 0 {
                out.push(AppEvent::InputState(
                    InputStateCommandEvent::UpdateMousePosition(pressure_pointer),
                ));
                if input.paint_button_down {
                    if input.orbit_modifier || input.alt {
                        out.push(AppEvent::Viewport(ViewportCommandEvent::Orbit {
                            dx: 0.0,
                            dy: 0.0,
                        }));
                    } else {
                        out.push(AppEvent::Tool(ToolCommandEvent::PointerMove(
                            pressure_pointer,
                        )));
                    }
                }
            } else {
                out.push(AppEvent::InputState(
                    InputStateCommandEvent::UpdateMousePosition(pressure_pointer),
                ));
                out.push(AppEvent::InputState(
                    InputStateCommandEvent::ResetPenPressure,
                ));
            }
        }
        WindowEvent::Touch(touch) => {
            let pressure = match touch.force {
                Some(winit::event::Force::Normalized(p)) => Some(p as f32),
                Some(winit::event::Force::Calibrated {
                    force,
                    max_possible_force,
                    ..
                }) => Some((force / max_possible_force) as f32),
                None => Some(1.0),
            };
            let pointer = pointer_from_position(
                input,
                touch.location,
                glam::Vec2::ZERO,
                PointerDeviceKind::Touch,
                pressure,
                match touch.phase {
                    TouchPhase::Started | TouchPhase::Moved => HoverState::Contact,
                    TouchPhase::Ended | TouchPhase::Cancelled => HoverState::Hovering,
                },
            );

            match touch.phase {
                TouchPhase::Started => {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer),
                    ));
                    if binding_matches_mouse(
                        &bindings.paint_button,
                        winit::event::MouseButton::Left,
                    ) {
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::SetPaintButtonDown(true),
                        ));
                        if !input.orbit_modifier && !input.alt {
                            out.push(AppEvent::Tool(ToolCommandEvent::PointerDown(pointer)));
                        }
                    }
                }
                TouchPhase::Moved => {
                    let dx = (touch.location.x - input.last_mouse_pos.x) as f32;
                    let dy = (touch.location.y - input.last_mouse_pos.y) as f32;
                    let mut pointer_with_delta = pointer;
                    pointer_with_delta.delta = glam::Vec2::new(dx, dy);

                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer_with_delta),
                    ));

                    if input.paint_button_down {
                        if input.orbit_modifier || input.alt {
                            out.push(AppEvent::Viewport(ViewportCommandEvent::Orbit { dx, dy }));
                        } else {
                            out.push(AppEvent::Tool(ToolCommandEvent::PointerMove(
                                pointer_with_delta,
                            )));
                        }
                    } else if input.pan_button_down {
                        out.push(AppEvent::Viewport(ViewportCommandEvent::Pan { dx, dy }));
                    }
                }
                TouchPhase::Ended => {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer),
                    ));
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::SetPaintButtonDown(false),
                    ));
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::ResetPenPressure,
                    ));
                    out.push(AppEvent::Tool(ToolCommandEvent::PointerUp(pointer)));
                }
                TouchPhase::Cancelled => {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer),
                    ));
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::SetPaintButtonDown(false),
                    ));
                    out.push(AppEvent::Tool(ToolCommandEvent::PointerCancel(pointer)));
                }
            }
        }
        WindowEvent::KeyboardInput { event, .. } => {
            if let PhysicalKey::Code(code) = event.physical_key {
                let is_pressed = event.state == ElementState::Pressed;

                out.push(AppEvent::InputState(
                    InputStateCommandEvent::UpdateModifier {
                        key: code,
                        is_pressed,
                    },
                ));

                if binding_matches_key(input, &bindings.orbit_modifier, code) {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::SetOrbitModifier(is_pressed),
                    ));
                } else if binding_matches_key(input, &bindings.pan_modifier, code) {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::SetAltModifier(is_pressed),
                    ));
                }

                if is_pressed {
                    if binding_matches_key(input, &bindings.undo, code) {
                        out.push(AppEvent::Ui(UiActionEvent::Undo));
                    } else if binding_matches_key(input, &bindings.redo, code) {
                        out.push(AppEvent::Ui(UiActionEvent::Redo));
                    } else if binding_matches_key(input, &bindings.brush_size_down, code) {
                        out.push(AppEvent::Ui(UiActionEvent::AdjustBrushSize(-5.0)));
                    } else if binding_matches_key(input, &bindings.brush_size_up, code) {
                        out.push(AppEvent::Ui(UiActionEvent::AdjustBrushSize(5.0)));
                    } else if binding_matches_key(input, &bindings.clear_canvas, code) {
                        out.push(AppEvent::Ui(UiActionEvent::ClearCanvas));
                    } else if binding_matches_key(input, &bindings.tool_brush, code) {
                        out.push(AppEvent::Ui(UiActionEvent::SelectTool(ToolKind::Brush)));
                    } else if binding_matches_key(input, &bindings.tool_eraser, code) {
                        out.push(AppEvent::Ui(UiActionEvent::SelectTool(ToolKind::Eraser)));
                    }
                }
            }
        }
        WindowEvent::MouseInput {
            state: button_state,
            button,
            ..
        } => {
            let pointer = pointer_from_position(
                input,
                input.last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                if *button_state == ElementState::Pressed {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );

            match button_state {
                ElementState::Pressed => {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer),
                    ));

                    if binding_matches_mouse(&bindings.paint_button, *button) {
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::SetPaintButtonDown(true),
                        ));
                        if !input.orbit_modifier && !input.alt {
                            out.push(AppEvent::Tool(ToolCommandEvent::PointerDown(pointer)));
                        }
                    } else if binding_matches_mouse(&bindings.pan_button, *button) {
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::SetPanButtonDown(true),
                        ));
                    }
                }
                ElementState::Released => {
                    out.push(AppEvent::InputState(
                        InputStateCommandEvent::UpdateMousePosition(pointer),
                    ));

                    if binding_matches_mouse(&bindings.paint_button, *button) {
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::SetPaintButtonDown(false),
                        ));
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::ResetPenPressure,
                        ));
                        out.push(AppEvent::Tool(ToolCommandEvent::PointerUp(pointer)));
                    } else if binding_matches_mouse(&bindings.pan_button, *button) {
                        out.push(AppEvent::InputState(
                            InputStateCommandEvent::SetPanButtonDown(false),
                        ));
                    }

                    if *button == MouseButton::Left {
                        out.push(AppEvent::Document(
                            DocumentCommandEvent::CommitCurrentStroke,
                        ));
                    }
                }
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            let delta = glam::Vec2::new(
                (position.x - input.last_mouse_pos.x) as f32,
                (position.y - input.last_mouse_pos.y) as f32,
            );
            let pointer = pointer_from_position(
                input,
                *position,
                delta,
                PointerDeviceKind::Mouse,
                None,
                if input.paint_button_down {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );

            out.push(AppEvent::InputState(
                InputStateCommandEvent::UpdateMousePosition(pointer),
            ));

            if input.paint_button_down {
                if input.orbit_modifier || input.alt {
                    out.push(AppEvent::Viewport(ViewportCommandEvent::Orbit {
                        dx: delta.x,
                        dy: delta.y,
                    }));
                } else {
                    out.push(AppEvent::Tool(ToolCommandEvent::PointerMove(pointer)));
                }
            } else if input.pan_button_down {
                out.push(AppEvent::Viewport(ViewportCommandEvent::Pan {
                    dx: delta.x,
                    dy: delta.y,
                }));
            }
        }
        WindowEvent::CursorEntered { .. } => {
            let pointer = pointer_from_position(
                input,
                input.last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                HoverState::Hovering,
            );
            out.push(AppEvent::InputState(
                InputStateCommandEvent::UpdateMousePosition(pointer),
            ));
        }
        WindowEvent::CursorLeft { .. } => {
            let pointer = pointer_from_position(
                input,
                input.last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                HoverState::Hovering,
            );
            out.push(AppEvent::InputState(
                InputStateCommandEvent::UpdateMousePosition(pointer),
            ));
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let scroll = match delta {
                MouseScrollDelta::LineDelta(_x, y) => *y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
            };
            out.push(AppEvent::Viewport(ViewportCommandEvent::Zoom { scroll }));
        }
        _ => {}
    }

    out
}
