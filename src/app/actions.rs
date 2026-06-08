use super::{State, Tool, ecs};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;

impl State {
    pub fn paint_at_cursor(&mut self) {
        if self.main_ui.egui_ctx.egui_wants_pointer_input() {
            return;
        }

        let start_time = std::time::Instant::now();

        let mouse_pos = glam::Vec2::new(
            self.app_state.input().last_mouse_pos.x as f32,
            self.app_state.input().last_mouse_pos.y as f32,
        );
        let screen_size = glam::Vec2::new(self.size.width as f32, self.size.height as f32);

        let (eye, view, proj, num_udim_tiles) = {
            let world = self.ecs_runtime.world();
            let camera = world.get_resource::<ecs::CameraResource>().expect("CameraResource");
            let doc = world.get_resource::<ecs::DocumentResource>().expect("DocumentResource");
            
            let eye = camera.get_eye();
            let view = glam::Mat4::look_at_rh(eye, camera.target, glam::Vec3::Y);
            let proj = glam::Mat4::perspective_rh(
                camera.fovy,
                camera.aspect,
                camera.znear,
                camera.zfar,
            );
            (eye, view, proj, doc.document.num_udim_tiles)
        };

        let ray = crate::raycast::Ray::from_screen(mouse_pos, screen_size, view, proj);

        let is_eraser = self.app_state.tool().active_tool == Tool::Eraser;

        // Apply tablet pressure to brush parameters
        let pressure = if self.app_state.input().has_tablet_input {
            self.calibrated_pressure()
        } else {
            1.0
        };
        let effective_size = self.app_state.canvas().brush_size * (0.2 + 0.8 * pressure); // Min 20% size at zero pressure
        let effective_opacity = self.app_state.canvas().brush_opacity * pressure;

        let mut brush_rgba = self.app_state.canvas().brush_color;
        brush_rgba[3] = (effective_opacity * 255.0) as u8;

        let raycast_start = std::time::Instant::now();
        let hit_opt = {
            let world = self.ecs_runtime.world();
            let doc = world.get_resource::<ecs::DocumentResource>().expect("DocumentResource");
            crate::raycast::intersect_document(&ray, &doc.document)
        };
        let raycast_duration = raycast_start.elapsed();

        if let Some(hit) = hit_opt {
            let paint_start = std::time::Instant::now();

            if self.interaction.stroke_in_progress.is_none() {
                self.interaction.stroke_in_progress = Some(crate::painter::PaintStroke {
                    points: Vec::new(),
                    uv_points: Vec::new(),
                    point_radii: Vec::new(),
                    point_alphas: Vec::new(),
                    color: brush_rgba,
                    radius: effective_size,
                    hardness: self.app_state.canvas().brush_hardness,
                    is_eraser,
                });
            }
            if let Some(ref mut stroke) = self.interaction.stroke_in_progress {
                stroke.points.push(hit.point);
                stroke.uv_points.push(hit.uv);
                stroke.point_radii.push(effective_size);
                stroke.point_alphas.push(brush_rgba[3]);
            }

            // Perform ECS queries for active layer views
            let world = self.ecs_runtime.world_mut();
            let mut active_query = world.query_filtered::<&ecs::LayerTexture, With<ecs::ActiveLayer>>();
            if let Ok(layer_tex) = active_query.get_single(world) {
                let view_refs: Vec<&wgpu::TextureView> = layer_tex.views.iter().map(|v| &**v).collect();
                let painter_res = world.get_resource::<ecs::PainterResource>().expect("PainterResource");
                let painter = &painter_res.0;

                if let Some(last_uv) = self.interaction.last_hit_uv {
                    painter.paint_stroke_to_views(
                        &self.device,
                        &self.queue,
                        &view_refs,
                        last_uv,
                        hit.uv,
                        self.interaction.last_hit_pos,
                        Some(hit.point),
                        brush_rgba,
                        effective_size,
                        self.app_state.canvas().brush_hardness,
                        is_eraser,
                        num_udim_tiles,
                    );
                } else {
                    log::info!(
                        "Stroke start coordinates:\n  Screen: [x: {:.2}, y: {:.2}]\n  World:  [x: {:.4}, y: {:.4}, z: {:.4}]\n  UV:     [u: {:.4}, v: {:.4}]\n  Pressure: {:.3}, Size: {:.1}",
                        mouse_pos.x,
                        mouse_pos.y,
                        hit.point.x,
                        hit.point.y,
                        hit.point.z,
                        hit.uv.x,
                        hit.uv.y,
                        pressure,
                        effective_size
                    );
                    painter.paint_stamp_to_views(
                        &self.device,
                        &self.queue,
                        &view_refs,
                        hit.uv,
                        Some(hit.point),
                        brush_rgba,
                        effective_size,
                        self.app_state.canvas().brush_hardness,
                        is_eraser,
                        num_udim_tiles,
                    );
                }
            }

            let paint_duration = paint_start.elapsed();
            self.interaction.last_hit_uv = Some(hit.uv);
            self.interaction.last_hit_pos = Some(hit.point);

            // Recompose layers
            self.ecs_runtime.compose_layers_only_ecs(&self.device, &self.queue);

            log::debug!(
                "Paint stroke timing: raycast={:?}, paint={:?}, total={:?}",
                raycast_duration,
                paint_duration,
                start_time.elapsed()
            );
            self.window.request_redraw();
        } else {
            self.interaction.last_hit_uv = None;
            self.interaction.last_hit_pos = None;
        }
    }

    pub fn toggle_mesh(&mut self, mode: &str) {
        let doc = match mode {
            "Cube" => crate::mesh::create_cube_document(
                &self.device,
                &self.viewport.node_bind_group_layout,
                2.0,
            ),
            "Plane" => crate::mesh::create_plane_document(
                &self.device,
                &self.viewport.node_bind_group_layout,
                2.5,
            ),
            _ => crate::mesh::create_sphere_document(
                &self.device,
                &self.viewport.node_bind_group_layout,
                1.5,
                32,
                32,
            ),
        };
        self.ecs_runtime.register_document(doc);
        self.viewport.update_node_transforms(self.ecs_runtime.world_mut(), &self.queue);
        self.ecs_runtime.reproject_strokes_ecs();
        self.ecs_runtime.redraw_all_layers_ecs(&self.device, &self.queue);
        log::info!("Switched geometry to {}", mode);
    }

    pub fn export_composite_texture(&self) {
        let path = "painted_texture.png";
        self.painter().export_png(&self.device, &self.queue, path);
    }

    pub fn load_gltf_file(&mut self, path: &std::path::Path) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.asset_loader.gltf_rx = Some(rx);
        self.emit_ui_action(ecs::events::UiActionEvent::StartGltfLoad);
        self.asset_loader.loading_path = Some(path.to_path_buf());

        // Extract strokes from non-fill layers to clone and reproject in background
        let mut strokes_to_reproject = Vec::new();
        {
            let mut world = self.ecs_runtime.world_mut();
            let mut query = world.query::<(&ecs::LayerIndex, &ecs::LayerStrokes, Option<&ecs::FillLayerProperties>)>();
            let mut layers: Vec<_> = query.iter(world).collect();
            layers.sort_by_key(|(idx, _, _)| idx.0);
            for (_, strokes, fill) in layers {
                if fill.is_none() {
                    strokes_to_reproject.push(strokes.0.clone());
                } else {
                    strokes_to_reproject.push(Vec::new());
                }
            }
        }

        let status = std::sync::Arc::new(std::sync::Mutex::new("Reading glTF file...".to_string()));
        self.asset_loader.gltf_loading_status = Some(status.clone());

        let path = path.to_path_buf();
        let device = self.device.clone();
        let layout = self.viewport.node_bind_group_layout.clone();
        let window = self.window.clone();

        std::thread::spawn(move || {
            if let Ok(mut lock) = status.lock() {
                *lock = "Loading meshes and textures...".to_string();
            }
            let res = crate::mesh::load_gltf(&device, &layout, &path).map(|doc| {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let mut reprojected_strokes = strokes_to_reproject;

                crate::painter::Painter::reproject_strokes_in_background(
                    &mut reprojected_strokes,
                    &doc,
                    &status,
                );

                (doc, filename, reprojected_strokes)
            });
            let _ = tx.send(res);
            window.request_redraw();
        });
    }

    pub fn focus_camera_on_model(&mut self) {
        let bounds_opt = {
            let world = self.ecs_runtime.world();
            let doc = world.get_resource::<ecs::DocumentResource>().expect("DocumentResource");
            doc.document.compute_bounds()
        };

        if let Some((min, max)) = bounds_opt {
            let center = (min + max) * 0.5;
            let size = max - min;
            let max_dim = size.x.max(size.y).max(size.z);

            let mut world = self.ecs_runtime.world_mut();
            if let Some(mut camera) = world.get_resource_mut::<ecs::CameraResource>() {
                camera.target = center;
                camera.distance = (max_dim * 1.5).max(1.0);
                log::info!(
                    "Focused camera at center: {:?}, distance: {}",
                    center,
                    camera.distance
                );
            }
        }
    }

    pub fn recompute_and_reproject(&mut self) {
        log::info!("Recomputing UV layout and reprojecting strokes...");
        {
            let mut world = self.ecs_runtime.world_mut();
            let mut doc = world.get_resource_mut::<ecs::DocumentResource>().expect("DocumentResource");
            doc.document.recompute_uvs(&self.import_settings, &self.device);
        }
        self.viewport.update_node_transforms(self.ecs_runtime.world_mut(), &self.queue);
        self.ecs_runtime.reproject_strokes_ecs();
        self.ecs_runtime.redraw_all_layers_ecs(&self.device, &self.queue);
        log::info!("UV layout recomputation and stroke reprojection complete!");
    }
}
