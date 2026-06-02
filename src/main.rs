mod app;
mod mesh;
mod viewport;
mod painter;
mod raycast;

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    state: Option<app::State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Antigravity DCC Painter")
                            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
                    )
                    .unwrap(),
            );
            let state = pollster::block_on(app::State::new(window));
            self.state = Some(state);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if let Some(ref mut state) = self.state {
            if window_id == state.window.id() {
                if state.input(&event) {
                    return;
                }

                match event {
                    WindowEvent::CloseRequested => event_loop.exit(),
                    WindowEvent::Resized(physical_size) => {
                        state.resize(physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { .. } => {
                        state.resize(state.window.inner_size());
                    }
                    WindowEvent::RedrawRequested => {
                        let size = state.window.inner_size();
                        if size.width > 0 && size.height > 0 {
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                Err(app::SurfaceError::Lost | app::SurfaceError::Outdated) => {
                                    state.resize(state.size);
                                }
                                Err(app::SurfaceError::Timeout) => {}
                                Err(app::SurfaceError::Other(e)) => {
                                    log::error!("Render error: {:?}", e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref state) = self.state {
            let size = state.window.inner_size();
            if size.width > 0 && size.height > 0 {
                state.window.request_redraw();
            }
        }
    }
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info,wgpu_hal::vulkan::conv=error"));
    log::info!("Starting Antigravity DCC Painter...");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    
    let mut app = App { state: None };
    event_loop.run_app(&mut app).unwrap();
}

