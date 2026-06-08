use super::{ecs, State, SurfaceError};

impl State {
    pub fn render_main_surface(
        &mut self,
        textures_delta: egui::TexturesDelta,
        paint_jobs: Vec<egui::ClippedPrimitive>,
    ) -> Result<(), SurfaceError> {
        for (id, image_delta) in &textures_delta.set {
            self.main_ui
                .egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        let surface_texture = {
            let main_ctx = self.ecs_runtime.world().get_resource::<ecs::MainRenderContextResource>().unwrap();
            match main_ctx.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) => t,
                wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                wgpu::CurrentSurfaceTexture::Timeout => return Err(SurfaceError::Timeout),
                wgpu::CurrentSurfaceTexture::Outdated => return Err(SurfaceError::Outdated),
                wgpu::CurrentSurfaceTexture::Lost => return Err(SurfaceError::Lost),
                wgpu::CurrentSurfaceTexture::Occluded => return Err(SurfaceError::Timeout),
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(SurfaceError::Other("Validation error".into()))
                }
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Main Render Encoder"),
            });

        self.main_ui.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        let painter_bind_group = self.painter().bind_group.clone();
        self.viewport.render(
            self.ecs_runtime.world_mut(),
            &mut encoder,
            &view,
            &painter_bind_group,
        );

        {
            let mut egui_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Egui Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                })
                .forget_lifetime();

            self.main_ui
                .egui_renderer
                .render(&mut egui_pass, &paint_jobs, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        for id in &textures_delta.free {
            self.main_ui.egui_renderer.free_texture(id);
        }

        self.ecs_runtime.ui_frame_ops.begin_main_egui_frame = false;
        self.ecs_runtime.ui_frame_ops.draw_main_egui_panels = false;
        self.ecs_runtime.ui_frame_ops.end_main_egui_frame_and_upload = false;
        self.main_ui.frame_begun = false;

        Ok(())
    }

    pub fn render_uv_surface(
        &mut self,
        textures_delta: egui::TexturesDelta,
        paint_jobs: Vec<egui::ClippedPrimitive>,
    ) -> Result<(), SurfaceError> {
        let viewer = match &mut self.uv_ui.viewer {
            Some(v) => v,
            None => return Ok(()),
        };

        for (id, image_delta) in &textures_delta.set {
            viewer
                .egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [viewer.config.width, viewer.config.height],
            pixels_per_point: viewer.window.scale_factor() as f32,
        };

        let surface_texture = match viewer.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(SurfaceError::Timeout),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(SurfaceError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(SurfaceError::Lost),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(SurfaceError::Timeout),
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(SurfaceError::Other("Validation error".into()))
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("UV Viewer Render Encoder"),
            });

        viewer.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let mut egui_pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("UV Viewer Egui Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.15,
                                g: 0.15,
                                b: 0.15,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                })
                .forget_lifetime();

            viewer
                .egui_renderer
                .render(&mut egui_pass, &paint_jobs, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        for id in &textures_delta.free {
            viewer.egui_renderer.free_texture(id);
        }

        self.ecs_runtime.ui_frame_ops.begin_uv_egui_frame = false;
        self.ecs_runtime.ui_frame_ops.draw_uv_egui_panels = false;
        self.ecs_runtime.ui_frame_ops.end_uv_egui_frame_and_upload = false;
        self.uv_ui.frame_begun = false;

        Ok(())
    }
}
