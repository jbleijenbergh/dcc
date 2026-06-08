use dcc_painter::app;

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
            match pollster::block_on(app::State::new(window)) {
                Ok(state) => {
                    self.state = Some(state);
                }
                Err(e) => {
                    log::error!("Failed to initialize application state: {}", e);
                    event_loop.exit();
                }
            }
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
                        state.queue_main_window_resize(physical_size.width, physical_size.height);
                    }
                    WindowEvent::ScaleFactorChanged { .. } => {
                        let size = state.window.inner_size();
                        state.queue_main_window_resize(size.width, size.height);
                    }
                    WindowEvent::RedrawRequested => {
                        let size = state.window.inner_size();
                        if size.width > 0 && size.height > 0 {
                            state.queue_main_redraw();
                            if let Err(app::SurfaceError::Other(e)) = state.update(event_loop) {
                                log::error!("Render error: {:?}", e);
                            }
                        }
                    }
                    _ => {}
                }
            } else if let Some(ref mut viewer) = state.uv_viewer {
                if window_id == viewer.window.id() {
                    let egui_resp = viewer.egui_state.on_window_event(&*viewer.window, &event);
                    if egui_resp.consumed {
                        return;
                    }
                    match event {
                        WindowEvent::CloseRequested => {
                            state.uv_viewer = None;
                            state.set_uv_viewer_visible(false);
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.queue_uv_window_resize(physical_size.width, physical_size.height);
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = viewer.window.inner_size();
                            state.queue_uv_window_resize(size.width, size.height);
                        }
                        WindowEvent::RedrawRequested => {
                            let size = viewer.window.inner_size();
                            if size.width > 0 && size.height > 0 {
                                state.queue_uv_redraw();
                                if let Err(e) = state.update(event_loop) {
                                    log::error!("UV viewer render error: {:?}", e);
                                }
                            }
                        }
                        _ => {}
                    }
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
            if let Some(ref viewer) = state.uv_viewer {
                let size = viewer.window.inner_size();
                if size.width > 0 && size.height > 0 {
                    viewer.window.request_redraw();
                }
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

