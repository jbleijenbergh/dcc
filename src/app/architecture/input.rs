use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::keyboard::PhysicalKey;

use super::message::Message;
use crate::app::State;

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

#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    PointerDown(PointerData),
    PointerMove(PointerData),
    PointerUp(PointerData),
    PointerCancel(PointerData),
    PointerEnter(PointerData),
    PointerLeave(PointerData),
    Wheel {
        delta: glam::Vec2,
        modifiers: ModifiersSnapshot,
    },
    KeyDown {
        key: winit::keyboard::KeyCode,
        modifiers: ModifiersSnapshot,
    },
    KeyUp {
        key: winit::keyboard::KeyCode,
        modifiers: ModifiersSnapshot,
    },
    ModifiersChanged(ModifiersSnapshot),
}

fn pointer_from_position(
    state: &State,
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
            primary: state.app_state.input().paint_button_down,
            secondary: state.app_state.input().pan_button_down,
            middle: false,
        },
        modifiers: ModifiersSnapshot {
            ctrl: state.app_state.input().ctrl,
            cmd: state.app_state.input().cmd,
            shift: state.app_state.input().shift,
            alt: state.app_state.input().alt,
        },
        hover_state,
        timestamp: std::time::Instant::now(),
    }
}

pub fn normalize_window_event(state: &State, event: &WindowEvent) -> Vec<Message> {
    let mut out = Vec::new();

    match event {
        WindowEvent::TouchpadPressure { pressure, stage, .. } => {
            let mut modifiers = ModifiersSnapshot {
                ctrl: state.app_state.input().ctrl,
                cmd: state.app_state.input().cmd,
                shift: state.app_state.input().shift,
                alt: state.app_state.input().alt,
            };
            if *stage <= 0 {
                modifiers.alt = state.app_state.input().alt;
            }
            out.push(Message::Input(InputEvent::ModifiersChanged(modifiers)));
            let pressure_pointer = pointer_from_position(
                state,
                state.app_state.input().last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Trackpad,
                Some(pressure.clamp(0.0, 1.0) as f32),
                if state.app_state.input().paint_button_down {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );
            out.push(Message::Input(if *stage > 0 {
                InputEvent::PointerMove(pressure_pointer)
            } else {
                InputEvent::PointerLeave(pressure_pointer)
            }));
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
                state,
                touch.location,
                glam::Vec2::ZERO,
                PointerDeviceKind::Touch,
                pressure,
                match touch.phase {
                    TouchPhase::Started | TouchPhase::Moved => HoverState::Contact,
                    TouchPhase::Ended | TouchPhase::Cancelled => HoverState::Hovering,
                },
            );
            out.push(Message::Input(match touch.phase {
                TouchPhase::Started => InputEvent::PointerDown(pointer),
                TouchPhase::Moved => InputEvent::PointerMove(pointer),
                TouchPhase::Ended => InputEvent::PointerUp(pointer),
                TouchPhase::Cancelled => InputEvent::PointerCancel(pointer),
            }));
        }
        WindowEvent::KeyboardInput { event, .. } => {
            if let PhysicalKey::Code(code) = event.physical_key {
                let modifiers = ModifiersSnapshot {
                    ctrl: state.app_state.input().ctrl,
                    cmd: state.app_state.input().cmd,
                    shift: state.app_state.input().shift,
                    alt: state.app_state.input().alt,
                };
                let input = match event.state {
                    ElementState::Pressed => InputEvent::KeyDown {
                        key: code,
                        modifiers,
                    },
                    ElementState::Released => InputEvent::KeyUp {
                        key: code,
                        modifiers,
                    },
                };
                out.push(Message::Input(input));
            }
        }
        WindowEvent::MouseInput { state: button_state, button, .. } => {
            let pointer = pointer_from_position(
                state,
                state.app_state.input().last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                if *button_state == ElementState::Pressed {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );
            let event = match button_state {
                ElementState::Pressed => InputEvent::PointerDown(pointer),
                ElementState::Released => InputEvent::PointerUp(pointer),
            };
            out.push(Message::Input(event));

            if *button == MouseButton::Left && *button_state == ElementState::Released {
                out.push(Message::Document(super::message::DocumentCommand::CommitCurrentStroke));
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            let delta = glam::Vec2::new(
                (position.x - state.app_state.input().last_mouse_pos.x) as f32,
                (position.y - state.app_state.input().last_mouse_pos.y) as f32,
            );
            let pointer = pointer_from_position(
                state,
                *position,
                delta,
                PointerDeviceKind::Mouse,
                None,
                if state.app_state.input().paint_button_down {
                    HoverState::Contact
                } else {
                    HoverState::Hovering
                },
            );
            out.push(Message::Input(InputEvent::PointerMove(pointer)));
        }
        WindowEvent::CursorEntered { .. } => {
            let pointer = pointer_from_position(
                state,
                state.app_state.input().last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                HoverState::Hovering,
            );
            out.push(Message::Input(InputEvent::PointerEnter(pointer)));
        }
        WindowEvent::CursorLeft { .. } => {
            let pointer = pointer_from_position(
                state,
                state.app_state.input().last_mouse_pos,
                glam::Vec2::ZERO,
                PointerDeviceKind::Mouse,
                None,
                HoverState::Hovering,
            );
            out.push(Message::Input(InputEvent::PointerLeave(pointer)));
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let scroll = match delta {
                MouseScrollDelta::LineDelta(x, y) => glam::Vec2::new(*x, *y),
                MouseScrollDelta::PixelDelta(p) => glam::Vec2::new(p.x as f32, p.y as f32 * 0.05),
            };
            out.push(Message::Input(InputEvent::Wheel {
                delta: scroll,
                modifiers: ModifiersSnapshot {
                    ctrl: state.app_state.input().ctrl,
                    cmd: state.app_state.input().cmd,
                    shift: state.app_state.input().shift,
                    alt: state.app_state.input().alt,
                },
            }));
        }
        _ => {}
    }

    out
}
