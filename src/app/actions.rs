use super::{State, Tool};

impl State {
    pub fn paint_at_cursor(&mut self) {
        if self.egui_ctx.egui_wants_pointer_input() {
            return;
        }

        let start_time = std::time::Instant::now();

        let mouse_pos = glam::Vec2::new(self.last_mouse_pos.x as f32, self.last_mouse_pos.y as f32);
        let screen_size = glam::Vec2::new(self.size.width as f32, self.size.height as f32);

        let eye = self.viewport.camera.get_eye();
        let view = glam::Mat4::look_at_rh(eye, self.viewport.camera.target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(
            self.viewport.camera.fovy,
            self.viewport.camera.aspect,
            self.viewport.camera.znear,
            self.viewport.camera.zfar,
        );

        let ray = crate::raycast::Ray::from_screen(mouse_pos, screen_size, view, proj);

        let is_eraser = self.active_tool == Tool::Eraser;
        let mut brush_rgba = self.brush_color;
        brush_rgba[3] = (self.brush_opacity * 255.0) as u8;

        let raycast_start = std::time::Instant::now();
        let hit_opt = crate::raycast::intersect_document(
            &ray,
            &self.viewport.document,
        );
        let raycast_duration = raycast_start.elapsed();

        if let Some(hit) = hit_opt {
            let paint_start = std::time::Instant::now();

            if self.current_stroke.is_none() && !is_eraser {
                self.current_stroke = Some(crate::painter::PaintStroke {
                    points: Vec::new(),
                    uv_points: Vec::new(),
                    color: brush_rgba,
                    radius: self.brush_size,
                    hardness: self.brush_hardness,
                    is_eraser: false,
                });
            }
            if let Some(ref mut stroke) = self.current_stroke {
                stroke.points.push(hit.point);
                stroke.uv_points.push(hit.uv);
            }

            if let Some(last_uv) = self.last_hit_uv {
                self.painter.paint_stroke(
                    &self.device,
                    &self.queue,
                    last_uv,
                    hit.uv,
                    brush_rgba,
                    self.brush_size,
                    self.brush_hardness,
                    is_eraser,
                );
            } else {
                log::info!(
                    "Mouse click / Stroke start coordinates:\n  Screen: [x: {:.2}, y: {:.2}]\n  World:  [x: {:.4}, y: {:.4}, z: {:.4}]\n  UV:     [u: {:.4}, v: {:.4}]",
                    mouse_pos.x,
                    mouse_pos.y,
                    hit.point.x,
                    hit.point.y,
                    hit.point.z,
                    hit.uv.x,
                    hit.uv.y
                );
                self.painter.paint_stamp(
                    &self.device,
                    &self.queue,
                    hit.uv,
                    brush_rgba,
                    self.brush_size,
                    self.brush_hardness,
                    is_eraser,
                );
            }
            let paint_duration = paint_start.elapsed();
            self.last_hit_uv = Some(hit.uv);

            log::info!(
                "Paint stroke timing: raycast={:?}, paint={:?}, total={:?}",
                raycast_duration,
                paint_duration,
                start_time.elapsed()
            );
        } else {
            self.last_hit_uv = None;
        }
    }

    pub fn toggle_mesh(&mut self, mode: &str) {
        let doc = match mode {
            "Cube" => crate::mesh::create_cube_document(&self.device, &self.viewport.node_bind_group_layout, 2.0),
            "Plane" => crate::mesh::create_plane_document(&self.device, &self.viewport.node_bind_group_layout, 2.5),
            _ => crate::mesh::create_sphere_document(&self.device, &self.viewport.node_bind_group_layout, 1.5, 32, 32),
        };
        self.viewport.set_document(doc);
        self.painter.reproject_strokes(&self.viewport.document);
        self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
        log::info!("Switched geometry to {}", mode);
    }

    pub fn export_composite_texture(&self) {
        let path = "painted_texture.png";
        self.painter.export_png(&self.device, &self.queue, path);
    }

    pub fn load_gltf_file(&mut self, path: &std::path::Path) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.gltf_rx = Some(rx);
        self.is_loading_gltf = true;
        self.loading_path = Some(path.to_path_buf());

        let path = path.to_path_buf();
        let device = self.device.clone();
        let layout = self.viewport.node_bind_group_layout.clone();
        let window = self.window.clone();

        std::thread::spawn(move || {
            let res = crate::mesh::load_gltf(&device, &layout, &path)
                .map(|doc| (doc, path.file_name().unwrap_or_default().to_string_lossy().to_string()));
            let _ = tx.send(res);
            window.request_redraw();
        });
    }

    pub fn focus_camera_on_model(&mut self) {
        if let Some((min, max)) = self.viewport.document.compute_bounds() {
            let center = (min + max) * 0.5;
            let size = max - min;
            let max_dim = size.x.max(size.y).max(size.z);
            
            self.viewport.camera.target = center;
            self.viewport.camera.distance = (max_dim * 1.5).max(1.0);
            log::info!("Focused camera at center: {:?}, distance: {}", center, self.viewport.camera.distance);
        }
    }

    pub fn recompute_and_reproject(&mut self) {
        log::info!("Recomputing UV layout and reprojecting strokes...");
        self.viewport.document.recompute_uvs(&self.import_settings, &self.device);
        self.painter.reproject_strokes(&self.viewport.document);
        self.painter.redraw_all_layers(&self.device, &self.queue, &self.viewport.document);
        log::info!("UV layout recomputation and stroke reprojection complete!");
    }
}
