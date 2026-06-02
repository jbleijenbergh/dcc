mod app;
mod mesh;
mod viewport;
mod painter;
mod raycast;

use std::sync::Arc;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info,wgpu_hal::vulkan::conv=error"));
    log::info!("Starting Antigravity DCC Painter...");

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Antigravity DCC Painter")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .build(&event_loop)
            .unwrap(),
    );

    let mut state = pollster::block_on(app::State::new(window.clone()));

    log::info!("Running event loop...");
    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                winit::event::Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == window.id() => {
                    if state.input(event) {
                        return;
                    }

                    match event {
                        winit::event::WindowEvent::CloseRequested => elwt.exit(),
                        winit::event::WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                        }
                        winit::event::WindowEvent::ScaleFactorChanged { .. } => {
                            state.resize(window.inner_size());
                        }
                        winit::event::WindowEvent::RedrawRequested => {
                            let size = window.inner_size();
                            if size.width > 0 && size.height > 0 {
                                state.update();
                                match state.render() {
                                    Ok(_) => {}
                                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                        state.resize(state.size);
                                    }
                                    Err(wgpu::SurfaceError::OutOfMemory) => {
                                        log::error!("OutOfMemory! Exiting.");
                                        elwt.exit();
                                    }
                                    Err(e) => {
                                        log::error!("Render error: {:?}", e);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                winit::event::Event::AboutToWait => {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        })
        .unwrap();
}
