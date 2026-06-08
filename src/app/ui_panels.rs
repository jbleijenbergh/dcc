use super::ecs;
use super::user_preferences;
use super::{State, SurfaceError, Tool};
use crate::painter::BlendMode;

impl State {
    #[allow(deprecated)]
    pub fn render(&mut self) -> Result<(), SurfaceError> {
        debug_assert!(
            self.main_ui.frame_begun,
            "Main UI frame was not begun by ECS lifecycle"
        );

        let mut close_error = false;
        let mut dismiss_error_requested = false;
        if let Some(ref err) = self.ui_state.error_details {
            let can_dismiss = self
                .ui_state
                .error_time
                .map_or(true, |t| t.elapsed().as_secs_f32() > 0.3);
            egui::Window::new("Error Loading Model")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(&self.main_ui.egui_ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚠️").size(24.0));
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Failed to load glTF model").strong());
                                ui.label(egui::RichText::new(err.path.file_name().unwrap_or_default().to_string_lossy()).weak());
                            });
                        });
                        ui.add_space(8.0);

                        ui.label(egui::RichText::new(&err.message).color(egui::Color32::from_rgb(255, 100, 100)));
                        ui.add_space(8.0);

                        let lower_msg = err.message.to_lowercase();
                        if lower_msg.contains("cannot find the path") || lower_msg.contains("os error 3") || lower_msg.contains("not found") {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("💡 Quick Suggestion:").strong().size(11.0));
                                ui.label(egui::RichText::new(
                                    "This glTF file references external assets (such as a separate .bin buffer or image textures) \
                                     that could not be found. Ensure all referenced files are in the same folder as the .gltf file."
                                ).size(10.5));
                            });
                            ui.add_space(8.0);
                        }

                        egui::CollapsingHeader::new("Technical Details")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(format!("File Path: {}", err.path.display())).weak().size(10.0));
                                ui.label(egui::RichText::new(format!("Raw Error: {}", err.message)).weak().size(10.0));
                            });

                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.add_enabled_ui(can_dismiss, |ui| {
                                if ui.button("OK").clicked() {
                                    close_error = true;
                                }
                            });
                        });
                    });
                });
        }
        if close_error {
            dismiss_error_requested = true;
        }

        let mut export_requested = false;
        let mut gltf_to_load = None;
        let mut save_bindings_requested = false;
        let mut reset_bindings_requested = false;
        let mut pending_ui_actions: Vec<ecs::events::UiActionEvent> = Vec::new();
        if dismiss_error_requested {
            pending_ui_actions.push(ecs::events::UiActionEvent::DismissLoadError);
        }
        let mut pending_tool_selection: Option<ecs::events::ToolKind> = None;
        let mut pending_brush_size: Option<f32> = None;
        let mut pending_brush_hardness: Option<f32> = None;
        let mut pending_brush_opacity: Option<f32> = None;
        let mut pending_brush_color: Option<[u8; 4]> = None;

        egui::Panel::top("top_menu").show(&self.main_ui.egui_ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open glTF Model...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("glTF Model", &["gltf", "glb"])
                            .pick_file()
                        {
                            gltf_to_load = Some(path);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Clear Canvas").clicked() {
                        pending_ui_actions.push(ecs::events::UiActionEvent::ClearCanvas);
                        ui.close();
                    }
                    if ui.button("Export Composed Texture (PNG)").clicked() {
                        export_requested = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let undo_enabled = !self.app_state.history().undo_stack.is_empty();
                    let undo_label = if undo_enabled {
                        "Undo (Ctrl+Z)"
                    } else {
                        "Undo"
                    };
                    if ui
                        .add_enabled(undo_enabled, egui::Button::new(undo_label))
                        .clicked()
                    {
                        pending_ui_actions.push(ecs::events::UiActionEvent::Undo);
                        ui.close();
                    }

                    let redo_enabled = !self.app_state.history().redo_stack.is_empty();
                    let redo_label = if redo_enabled {
                        "Redo (Ctrl+Y)"
                    } else {
                        "Redo"
                    };
                    if ui
                        .add_enabled(redo_enabled, egui::Button::new(redo_label))
                        .clicked()
                    {
                        pending_ui_actions.push(ecs::events::UiActionEvent::Redo);
                        ui.close();
                    }
                });

                ui.menu_button("Window", |ui| {
                    if ui
                        .selectable_label(self.app_state.ui().show_uv_viewer, "UV Viewer")
                        .clicked()
                    {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetUvViewerVisible(
                            !self.app_state.ui().show_uv_viewer,
                        ));
                        ui.close();
                    }
                });

                ui.separator();
                if ui
                    .selectable_label(self.app_state.ui().show_uv_viewer, "🗺 View UVs")
                    .clicked()
                {
                    pending_ui_actions.push(ecs::events::UiActionEvent::SetUvViewerVisible(
                        !self.app_state.ui().show_uv_viewer,
                    ));
                }

                ui.separator();
                ui.label("Model:");
                let current_mesh = self.app_state.document().current_mesh.clone();
                let mut selected_mesh = current_mesh.clone();
                egui::ComboBox::from_id_salt("mesh_select")
                    .selected_text(&current_mesh)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected_mesh, "Sphere".to_string(), "Sphere");
                        ui.selectable_value(&mut selected_mesh, "Cube".to_string(), "Cube");
                        ui.selectable_value(&mut selected_mesh, "Plane".to_string(), "Plane");
                    });
                if selected_mesh != current_mesh {
                    pending_ui_actions.push(ecs::events::UiActionEvent::SwitchMesh(selected_mesh));
                }

                if self.viewport.document.scenes.len() > 1 {
                    ui.separator();
                    ui.label("Scene:");
                    let active_scene_idx = self.viewport.document.active_scene_idx;
                    let scene_name = self.viewport.document.scenes[active_scene_idx]
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Scene {}", active_scene_idx));
                    let mut selected_scene_idx = active_scene_idx;
                    egui::ComboBox::from_id_salt("scene_select")
                        .selected_text(&scene_name)
                        .show_ui(ui, |ui| {
                            for (idx, _scene) in self.viewport.document.scenes.iter().enumerate() {
                                let name = _scene
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| format!("Scene {}", idx));
                                ui.selectable_value(&mut selected_scene_idx, idx, name);
                            }
                        });
                    if selected_scene_idx != active_scene_idx {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetActiveScene(
                            selected_scene_idx,
                        ));
                        log::info!("Switched active scene to index {}", selected_scene_idx);
                    }
                }
            });
        });

        egui::Panel::bottom("asset_shelf")
            .resizable(true)
            .min_size(60.0)
            .show(&self.main_ui.egui_ctx, |ui| {
                ui.heading("Asset Shelf");
                ui.horizontal(|ui| {
                    ui.label("Assets will appear here...");
                });
            });

        egui::Panel::left("left_toolbar")
            .resizable(false)
            .show(&self.main_ui.egui_ctx, |ui| {
                ui.heading("Tools");
                ui.separator();

                let brush_btn = ui.selectable_label(
                    self.app_state.tool().active_tool == Tool::Brush,
                    format!("{} Brush", egui_phosphor::regular::PAINT_BRUSH),
                );
                if brush_btn.clicked() {
                    pending_tool_selection = Some(ecs::events::ToolKind::Brush);
                }

                let eraser_btn = ui.selectable_label(
                    self.app_state.tool().active_tool == Tool::Eraser,
                    format!("{} Eraser", egui_phosphor::regular::ERASER),
                );
                if eraser_btn.clicked() {
                    pending_tool_selection = Some(ecs::events::ToolKind::Eraser);
                }

                ui.separator();
                if ui.button("Clear All").clicked() {
                    pending_ui_actions.push(ecs::events::UiActionEvent::ClearCanvas);
                }
            });

        egui::Panel::right("right_panel").default_size(280.0).show(&self.main_ui.egui_ctx, |ui| {
            ui.heading("Settings");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Brush Settings")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        let mut brush_size = self.app_state.canvas().brush_size;
                        if ui
                            .add(egui::Slider::new(&mut brush_size, 2.0..=300.0).text("px"))
                            .changed()
                        {
                            pending_brush_size = Some(brush_size);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Hardness:");
                        let mut brush_hardness = self.app_state.canvas().brush_hardness;
                        if ui
                            .add(egui::Slider::new(&mut brush_hardness, 0.0..=1.0))
                            .changed()
                        {
                            pending_brush_hardness = Some(brush_hardness);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        let mut brush_opacity = self.app_state.canvas().brush_opacity;
                        if ui
                            .add(egui::Slider::new(&mut brush_opacity, 0.0..=1.0))
                            .changed()
                        {
                            pending_brush_opacity = Some(brush_opacity);
                        }
                    });

                    ui.separator();
                    if ui.button("Calibrate Pressure…").clicked() {
                        self.app_state.ui_mut().show_pressure_calibration = true;
                    }

                    ui.separator();
                    ui.label("Color:");

                    let mut color_f32 = [
                        self.app_state.canvas().brush_color[0] as f32 / 255.0,
                        self.app_state.canvas().brush_color[1] as f32 / 255.0,
                        self.app_state.canvas().brush_color[2] as f32 / 255.0,
                        self.app_state.canvas().brush_color[3] as f32 / 255.0,
                    ];

                    if ui.color_edit_button_rgba_unmultiplied(&mut color_f32).changed() {
                        let brush_color = [
                            (color_f32[0] * 255.0) as u8,
                            (color_f32[1] * 255.0) as u8,
                            (color_f32[2] * 255.0) as u8,
                            (color_f32[3] * 255.0) as u8,
                        ];
                        pending_brush_color = Some(brush_color);
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Layers")
                .default_open(true)
                .show(ui, |ui| {
                    let layer_name_id = egui::Id::new("add_layer_name_draft");
                    let mut layer_name_draft = self
                        .main_ui.egui_ctx
                        .data_mut(|d| d.get_persisted::<String>(layer_name_id).unwrap_or_default());

                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut layer_name_draft);
                        if ui.button("Add").clicked() {
                            let layer_name = layer_name_draft.trim().to_string();
                            if !layer_name.is_empty() {
                                pending_ui_actions.push(ecs::events::UiActionEvent::AddPaintLayer(layer_name));
                                layer_name_draft.clear();
                            }
                        }
                    });

                    self.main_ui.egui_ctx
                        .data_mut(|d| d.insert_persisted(layer_name_id, layer_name_draft));

                    ui.horizontal(|ui| {
                        if ui.button("Add UV Grid Layer").clicked() {
                            pending_ui_actions.push(ecs::events::UiActionEvent::AddUvGridLayer);
                        }
                        if ui.button("Add UV Checker Layer").clicked() {
                            pending_ui_actions.push(ecs::events::UiActionEvent::AddUvCheckerLayer);
                        }
                    });

                    if ui.button("✨ Add Fill Layer").clicked() {
                        pending_ui_actions.push(ecs::events::UiActionEvent::AddFillLayer);
                    }

                    ui.separator();

                    let layer_count = self.painter.layers.len();

                    for idx in (0..layer_count).rev() {
                        let is_active = self.painter.active_layer_idx == idx;
                        ui.horizontal(|ui| {
                            if ui.selectable_label(is_active, &self.painter.layers[idx].name).clicked() {
                                pending_ui_actions.push(ecs::events::UiActionEvent::SelectLayer(idx));
                            }

                            let mut visible = self.painter.layers[idx].visible;
                            if ui.checkbox(&mut visible, "").changed() {
                                pending_ui_actions.push(ecs::events::UiActionEvent::SetLayerVisible { idx, visible });
                            }

                            if layer_count > 1 {
                                if ui.button("🗑").clicked() {
                                    pending_ui_actions.push(ecs::events::UiActionEvent::DeleteLayer(idx));
                                }
                            }
                        });

                        if is_active {
                            let is_fill = self.painter.layers[idx].is_fill;
                            ui.indent("layer_props", |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Blend:");
                                    let mut blend = self.painter.layers[idx].blend_mode;
                                    egui::ComboBox::from_id_salt(format!("blend_{}", idx))
                                        .selected_text(blend.to_str())
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_value(&mut blend, BlendMode::Normal, "Normal").changed() {
                                                pending_ui_actions.push(ecs::events::UiActionEvent::SetLayerBlendMode { idx, mode: blend });
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Multiply, "Multiply").changed() {
                                                pending_ui_actions.push(ecs::events::UiActionEvent::SetLayerBlendMode { idx, mode: blend });
                                            }
                                            if ui.selectable_value(&mut blend, BlendMode::Add, "Add").changed() {
                                                pending_ui_actions.push(ecs::events::UiActionEvent::SetLayerBlendMode { idx, mode: blend });
                                            }
                                        });
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    let mut op = self.painter.layers[idx].opacity;
                                    let response = ui.add(egui::Slider::new(&mut op, 0.0..=1.0));
                                    if response.changed() {
                                        pending_ui_actions.push(ecs::events::UiActionEvent::SetLayerOpacity {
                                            idx,
                                            opacity: op,
                                            begin_undo: response.drag_started(),
                                        });
                                    }
                                });

                                if is_fill {
                                    ui.separator();
                                    ui.label(egui::RichText::new("Fill Layer Settings").strong());

                                    ui.horizontal(|ui| {
                                        ui.label("Base Color:");
                                        let mut c = [
                                            self.painter.layers[idx].fill_color[0] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[1] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[2] as f32 / 255.0,
                                            self.painter.layers[idx].fill_color[3] as f32 / 255.0,
                                        ];
                                        let response = ui.color_edit_button_rgba_unmultiplied(&mut c);
                                        if response.changed() {
                                            let color = [
                                                (c[0] * 255.0) as u8, (c[1] * 255.0) as u8,
                                                (c[2] * 255.0) as u8, (c[3] * 255.0) as u8,
                                            ];
                                            pending_ui_actions.push(ecs::events::UiActionEvent::SetFillBaseColor {
                                                idx,
                                                color,
                                                begin_undo: response.drag_started(),
                                            });
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Noise Color:");
                                        let mut c = [
                                            self.painter.layers[idx].fill_noise_color[0] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[1] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[2] as f32 / 255.0,
                                            self.painter.layers[idx].fill_noise_color[3] as f32 / 255.0,
                                        ];
                                        let response = ui.color_edit_button_rgba_unmultiplied(&mut c);
                                        if response.changed() {
                                            let color = [
                                                (c[0] * 255.0) as u8, (c[1] * 255.0) as u8,
                                                (c[2] * 255.0) as u8, (c[3] * 255.0) as u8,
                                            ];
                                            pending_ui_actions.push(ecs::events::UiActionEvent::SetFillNoiseColor {
                                                idx,
                                                color,
                                                begin_undo: response.drag_started(),
                                            });
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Noise Scale:");
                                        let mut scale = self.painter.layers[idx].fill_noise_scale;
                                        let response = ui.add(egui::Slider::new(&mut scale, 0.5..=50.0));
                                        if response.changed() {
                                            pending_ui_actions.push(ecs::events::UiActionEvent::SetFillNoiseScale {
                                                idx,
                                                scale,
                                                begin_undo: response.drag_started(),
                                            });
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Projection:");
                                        let mut mode = self.painter.layers[idx].fill_projection_mode;
                                        let prev_mode = mode;
                                        egui::ComboBox::from_id_salt(format!("proj_{}", idx))
                                            .selected_text(if mode == 1 { "Triplanar" } else { "UV" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut mode, 0u32, "UV");
                                                ui.selectable_value(&mut mode, 1u32, "Triplanar");
                                            });
                                        if mode != prev_mode {
                                            pending_ui_actions.push(ecs::events::UiActionEvent::SetFillProjectionMode { idx, mode });
                                        }
                                    });
                                }
                            });
                        }
                        ui.separator();
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("UV Settings")
                .default_open(false)
                .show(ui, |ui| {
                    let mut seams_option = self.import_settings.seams_option;
                    ui.horizontal(|ui| {
                        ui.label("Seams:");
                        egui::ComboBox::from_id_salt("seams_select")
                            .selected_text(match seams_option {
                                crate::mesh::SeamsOption::GenerateMissing => "Generate Missing",
                                crate::mesh::SeamsOption::RecomputeAll => "Recompute All",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut seams_option,
                                    crate::mesh::SeamsOption::GenerateMissing,
                                    "Generate Missing",
                                );
                                ui.selectable_value(
                                    &mut seams_option,
                                    crate::mesh::SeamsOption::RecomputeAll,
                                    "Recompute All",
                                );
                            });
                    });
                    if seams_option != self.import_settings.seams_option {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetImportSeams(seams_option));
                    }

                    let mut margin_size = self.import_settings.margin_size;
                    ui.horizontal(|ui| {
                        ui.label("Margin:");
                        egui::ComboBox::from_id_salt("margin_select")
                            .selected_text(match margin_size {
                                crate::mesh::MarginSize::Small => "Small",
                                crate::mesh::MarginSize::Medium => "Medium",
                                crate::mesh::MarginSize::Large => "Large",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut margin_size,
                                    crate::mesh::MarginSize::Small,
                                    "Small",
                                );
                                ui.selectable_value(
                                    &mut margin_size,
                                    crate::mesh::MarginSize::Medium,
                                    "Medium",
                                );
                                ui.selectable_value(
                                    &mut margin_size,
                                    crate::mesh::MarginSize::Large,
                                    "Large",
                                );
                            });
                    });
                    if margin_size != self.import_settings.margin_size {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetImportMargin(margin_size));
                    }

                    let mut island_orientation = self.import_settings.island_orientation;
                    ui.horizontal(|ui| {
                        ui.label("Orientation:");
                        egui::ComboBox::from_id_salt("orientation_select")
                            .selected_text(match island_orientation {
                                crate::mesh::IslandOrientation::AlignWith3DMesh => "Align with 3D Mesh",
                                crate::mesh::IslandOrientation::Unconstrained => "Unconstrained",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut island_orientation,
                                    crate::mesh::IslandOrientation::AlignWith3DMesh,
                                    "Align with 3D Mesh",
                                );
                                ui.selectable_value(
                                    &mut island_orientation,
                                    crate::mesh::IslandOrientation::Unconstrained,
                                    "Unconstrained",
                                );
                            });
                    });
                    if island_orientation != self.import_settings.island_orientation {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetImportOrientation(island_orientation));
                    }

                    ui.add_space(4.0);
                    if ui.button("🔄 Recompute UVs & Reproject Strokes").clicked() {
                        pending_ui_actions.push(ecs::events::UiActionEvent::RecomputeUvsAndReproject);
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Model Hierarchy")
                .default_open(true)
                .show(ui, |ui| {
                    let doc = &self.viewport.document;
                    if doc.scenes.is_empty() {
                        ui.label("No scene loaded");
                    } else {
                        let active_scene = &doc.scenes[doc.active_scene_idx];
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for &root_idx in &active_scene.root_nodes {
                                super::ui::draw_node_tree(ui, &doc.nodes, root_idx, &doc.materials);
                            }
                        });
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Materials")
                .default_open(true)
                .show(ui, |ui| {
                    let doc = &self.viewport.document;
                    if doc.materials.is_empty() {
                        ui.label(egui::RichText::new("No materials in model").weak().italics());
                    } else {
                        let mut unique_materials = Vec::new();
                        for (idx, mat) in doc.materials.iter().enumerate() {
                            let is_duplicate = if let Some(gltf_id) = mat.gltf_index {
                                unique_materials.iter().any(|(_, m): &(usize, &crate::mesh::MaterialInfo)| m.gltf_index == Some(gltf_id))
                            } else {
                                unique_materials.iter().any(|(_, m): &(usize, &crate::mesh::MaterialInfo)| m.name == mat.name && m.gltf_index.is_none())
                            };
                            if !is_duplicate {
                                unique_materials.push((idx, mat));
                            }
                        }

                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (idx, mat) in unique_materials {
                                super::ui::draw_material_details(ui, idx, mat);
                            }
                        });
                    }
                });

            ui.separator();
            egui::CollapsingHeader::new("Light Settings")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Light Angle:");
                        ui.add(egui::Slider::new(&mut self.viewport.light_angle, 0.0..=std::f32::consts::PI * 2.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Light Intensity:");
                        ui.add(egui::Slider::new(&mut self.viewport.light_intensity, 0.0..=5.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ambient Strength:");
                        ui.add(egui::Slider::new(&mut self.viewport.ambient_strength, 0.0..=1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("View Transform:");
                        let current_transform = self.viewport.view_transform;
                        egui::ComboBox::from_id_salt("view_transform_select")
                            .selected_text(match current_transform {
                                crate::viewport::ViewTransform::Standard => "Standard Linear",
                                crate::viewport::ViewTransform::AgX => "AgX",
                                crate::viewport::ViewTransform::ACES => "ACES",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::Standard, "Standard Linear");
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::AgX, "AgX");
                                ui.selectable_value(&mut self.viewport.view_transform, crate::viewport::ViewTransform::ACES, "ACES");
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Exposure:");
                        ui.add(egui::Slider::new(&mut self.viewport.exposure, 0.1..=5.0));
                    });
                });

            ui.separator();
            egui::CollapsingHeader::new("Display Info")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Color Format:");
                        ui.label(egui::RichText::new(format!("{:?}", self.config.format)).strong());
                    });

                    let refresh_rate = self.window.current_monitor()
                        .and_then(|m| m.refresh_rate_millihertz())
                        .map(|mhz| format!("{:.1} Hz", mhz as f32 / 1000.0))
                        .unwrap_or_else(|| "Unknown".to_string());
                    ui.horizontal(|ui| {
                        ui.label("Refresh Rate:");
                        ui.label(egui::RichText::new(refresh_rate).strong());
                    });

                    ui.horizontal(|ui| {
                        ui.label("VSync / Present Mode:");
                        let mut present_mode = self.config.present_mode;
                        let prev_mode = present_mode;
                        egui::ComboBox::from_id_salt("present_mode_select")
                            .selected_text(match present_mode {
                                wgpu::PresentMode::Fifo => "VSync On (Fifo)",
                                wgpu::PresentMode::Immediate => "VSync Off (Immediate)",
                                wgpu::PresentMode::Mailbox => "VSync Off (Mailbox)",
                                wgpu::PresentMode::AutoVsync => "Auto VSync",
                                wgpu::PresentMode::AutoNoVsync => "Auto VSync Off",
                                _ => "Other",
                            })
                            .show_ui(ui, |ui| {
                                if self.render_host.supports_present_mode(wgpu::PresentMode::Fifo) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Fifo, "VSync On (Fifo)");
                                }
                                if self.render_host.supports_present_mode(wgpu::PresentMode::Immediate) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Immediate, "VSync Off (Immediate)");
                                }
                                if self.render_host.supports_present_mode(wgpu::PresentMode::Mailbox) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::Mailbox, "VSync Off (Mailbox)");
                                }
                                if self.render_host.supports_present_mode(wgpu::PresentMode::AutoVsync) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::AutoVsync, "Auto VSync");
                                }
                                if self.render_host.supports_present_mode(wgpu::PresentMode::AutoNoVsync) {
                                    ui.selectable_value(&mut present_mode, wgpu::PresentMode::AutoNoVsync, "Auto VSync Off");
                                }
                            });
                        if present_mode != prev_mode {
                            self.config.present_mode = present_mode;
                            self.surface.configure(&self.device, &self.config);
                            log::info!("Present mode switched to {:?}", present_mode);
                        }
                    });
                });

            ui.separator();
            egui::CollapsingHeader::new("Bindings")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Customize keys and mouse buttons for actions.");
            ui.small(format!("Settings file: {}", self.preferences_path.display()));
                    ui.separator();
                    ui.label(egui::RichText::new("Navigation").strong());
                    ui.group(|ui| {
                        ui.label("Orbit Modifier");
                        Self::draw_key_binding_editor(ui, "bind_orbit", &mut self.preferences.bindings.orbit_modifier);
                    });
                    ui.group(|ui| {
                        ui.label("Pan Modifier");
                        Self::draw_key_binding_editor(ui, "bind_pan_mod", &mut self.preferences.bindings.pan_modifier);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Edit Actions").strong());
                    ui.group(|ui| {
                        ui.label("Undo");
                        Self::draw_key_binding_editor(ui, "bind_undo", &mut self.preferences.bindings.undo);
                    });
                    ui.group(|ui| {
                        ui.label("Redo");
                        Self::draw_key_binding_editor(ui, "bind_redo", &mut self.preferences.bindings.redo);
                    });
                    ui.group(|ui| {
                        ui.label("Clear Canvas");
                        Self::draw_key_binding_editor(ui, "bind_clear", &mut self.preferences.bindings.clear_canvas);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Brush").strong());
                    ui.group(|ui| {
                        ui.label("Brush Size Down");
                        Self::draw_key_binding_editor(ui, "bind_size_down", &mut self.preferences.bindings.brush_size_down);
                    });
                    ui.group(|ui| {
                        ui.label("Brush Size Up");
                        Self::draw_key_binding_editor(ui, "bind_size_up", &mut self.preferences.bindings.brush_size_up);
                    });
                    ui.group(|ui| {
                        ui.label("Select Brush Tool");
                        Self::draw_key_binding_editor(ui, "bind_tool_brush", &mut self.preferences.bindings.tool_brush);
                    });
                    ui.group(|ui| {
                        ui.label("Select Eraser Tool");
                        Self::draw_key_binding_editor(ui, "bind_tool_eraser", &mut self.preferences.bindings.tool_eraser);
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Mouse Buttons").strong());
                    ui.group(|ui| {
                        ui.label("Paint Button");
                        Self::draw_mouse_binding_editor(ui, "bind_paint_btn", &mut self.preferences.bindings.paint_button);
                    });
                    ui.group(|ui| {
                        ui.label("Pan Button");
                        Self::draw_mouse_binding_editor(ui, "bind_pan_btn", &mut self.preferences.bindings.pan_button);
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Save Bindings").clicked() {
                            save_bindings_requested = true;
                        }
                        if ui.button("Reset Defaults").clicked() {
                            reset_bindings_requested = true;
                        }
                    });

                    let conflicts = Self::binding_conflicts(&self.preferences.bindings);
                    if !conflicts.is_empty() {
                        ui.add_space(6.0);
                        ui.colored_label(egui::Color32::from_rgb(255, 180, 80), "Binding conflicts detected:");
                        for conflict in conflicts {
                            ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("- {}", conflict));
                        }
                    }

                    if let Some(msg) = &self.ui_state.settings_feedback {
                        ui.label(egui::RichText::new(msg).small());
                    }
                });
            });
        });

        if let Some(tool) = pending_tool_selection {
            self.emit_ui_action(ecs::events::UiActionEvent::SelectTool(tool));
        }
        if let Some(size) = pending_brush_size {
            self.emit_ui_action(ecs::events::UiActionEvent::SetBrushSize(size));
        }
        if let Some(hardness) = pending_brush_hardness {
            self.emit_ui_action(ecs::events::UiActionEvent::SetBrushHardness(hardness));
        }
        if let Some(opacity) = pending_brush_opacity {
            self.emit_ui_action(ecs::events::UiActionEvent::SetBrushOpacity(opacity));
        }
        if let Some(color) = pending_brush_color {
            self.emit_ui_action(ecs::events::UiActionEvent::SetBrushColor(color));
        }
        if reset_bindings_requested {
            self.preferences.bindings = user_preferences::InputBindings::default();
            self.save_settings();
        } else if save_bindings_requested {
            self.save_settings();
        }

        if self.app_state.ui().show_pressure_calibration {
            let mut is_open = self.app_state.ui().show_pressure_calibration;
            let egui_ctx = self.main_ui.egui_ctx.clone();
            egui::Window::new("Pressure Calibration")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut is_open)
                .show(&egui_ctx, |ui| {
                    ui.label("Adjust pressure response curve");
                    ui.add_space(4.0);

                    let mut min_start = self.preferences.pressure_curve_min_start;
                    let mut max_at = self.preferences.pressure_curve_max_at;

                    ui.horizontal(|ui| {
                        ui.label("Min start");
                        ui.add(egui::Slider::new(&mut min_start, 0.0..=1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max at");
                        ui.add(egui::Slider::new(&mut max_at, 0.0..=1.0));
                    });

                    if min_start >= max_at {
                        if min_start == self.preferences.pressure_curve_min_start {
                            max_at = (min_start + 0.001).min(1.0);
                        } else {
                            min_start = (max_at - 0.001).max(0.0);
                        }
                    }
                    if min_start != self.preferences.pressure_curve_min_start
                        || max_at != self.preferences.pressure_curve_max_at
                    {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetPressureCurve {
                            min_start,
                            max_at,
                        });
                    }

                    let calibrated = self.calibrated_pressure();
                    ui.separator();
                    ui.label(format!(
                        "Current pressure: {:.3} (mapped: {:.3})",
                        self.app_state.input().pen_pressure,
                        calibrated
                    ));
                    if self.app_state.input().touchpad_pressure_stage > 0 {
                        ui.label(format!(
                            "Force click stage: {}",
                            self.app_state.input().touchpad_pressure_stage
                        ));
                    }

                    let graph_size = egui::vec2(320.0, 140.0);
                    let (rect, _) = ui.allocate_exact_size(graph_size, egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    let bg = egui::Color32::from_gray(20);
                    let stroke = egui::Stroke::new(1.0, egui::Color32::GRAY);
                    painter.rect_filled(rect, 4.0, bg);
                    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);

                    let to_screen = |x: f32, y: f32| -> egui::Pos2 {
                        egui::pos2(
                            egui::lerp(rect.left()..=rect.right(), x.clamp(0.0, 1.0)),
                            egui::lerp(rect.bottom()..=rect.top(), y.clamp(0.0, 1.0)),
                        )
                    };

                    let line_color = egui::Color32::from_rgb(120, 200, 255);
                    let curve = vec![
                        to_screen(0.0, 0.0),
                        to_screen(self.preferences.pressure_curve_min_start, 0.0),
                        to_screen(self.preferences.pressure_curve_max_at, 1.0),
                        to_screen(1.0, 1.0),
                    ];
                    painter.add(egui::Shape::line(curve, egui::Stroke::new(2.0, line_color)));

                    let marker = to_screen(self.app_state.input().pen_pressure, calibrated);
                    painter.circle_filled(marker, 4.0, egui::Color32::YELLOW);
                });
            self.app_state.ui_mut().show_pressure_calibration = is_open;
        }

        for action in pending_ui_actions {
            self.emit_ui_action(action);
        }
        self.process_ecs_step();

        if export_requested {
            self.export_composite_texture();
        }

        if let Some(path) = gltf_to_load {
            self.load_gltf_file(&path);
        }

        if self.app_state.resources().is_loading_gltf {
            let status_msg = self
                .asset_loader
                .gltf_loading_status
                .as_ref()
                .and_then(|status| status.lock().ok())
                .map(|g| g.clone())
                .unwrap_or_else(|| {
                    "Reading and compiling model resources in the background".to_string()
                });

            egui::Window::new("Loading Model")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .movable(false)
                .title_bar(false)
                .frame(
                    egui::Frame::window(&self.main_ui.egui_ctx.global_style())
                        .fill(egui::Color32::from_black_alpha(200))
                        .inner_margin(25.0)
                        .corner_radius(12.0),
                )
                .show(&self.main_ui.egui_ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(50.0));
                        ui.add_space(15.0);
                        ui.label(
                            egui::RichText::new("Loading glTF Model...")
                                .size(18.0)
                                .color(egui::Color32::WHITE)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(status_msg)
                                .size(12.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                    });
                });
        }

        let egui_output = self.main_ui.egui_ctx.end_pass();
        let paint_jobs = self
            .main_ui
            .egui_ctx
            .tessellate(egui_output.shapes, egui_output.pixels_per_point);

        for (id, image_delta) in &egui_output.textures_delta.set {
            self.main_ui
                .egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        let surface_texture = match self.surface.get_current_texture() {
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
                label: Some("Main Render Encoder"),
            });

        self.main_ui.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        self.viewport.render(
            &mut encoder,
            &view,
            &self.depth_view,
            &self.painter.bind_group,
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

        for id in &egui_output.textures_delta.free {
            self.main_ui.egui_renderer.free_texture(id);
        }

        self.ecs_runtime.ui_frame_ops.begin_main_egui_frame = false;
        self.ecs_runtime.ui_frame_ops.draw_main_egui_panels = false;
        self.ecs_runtime.ui_frame_ops.end_main_egui_frame_and_upload = false;
        self.main_ui.frame_begun = false;

        Ok(())
    }

    pub fn render_uv_viewer(&mut self) -> Result<(), SurfaceError> {
        let mut pending_ui_actions: Vec<ecs::events::UiActionEvent> = Vec::new();
        {
            let viewer = match &mut self.uv_ui.viewer {
                Some(v) => v,
                None => return Ok(()),
            };

            debug_assert!(
                self.uv_ui.frame_begun,
                "UV UI frame was not begun by ECS lifecycle"
            );

            let num_tiles = self.viewport.document.num_udim_tiles.max(1) as usize;
            let show_uv_wireframe = self.app_state.ui().show_uv_wireframe;
            let active_nodes = self.viewport.document.get_active_nodes();

            #[allow(deprecated)]
            egui::CentralPanel::default().show(&viewer.egui_ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Source:");

                    let current_source = self.app_state.ui().uv_viewer_source;
                    let mut selected_source = current_source;
                    let source_name = if current_source == 0 {
                        "Composed Result".to_string()
                    } else {
                        let idx = current_source - 1;
                        if idx < self.painter.layers.len() {
                            format!("Layer: {}", self.painter.layers[idx].name)
                        } else {
                            "Unknown Layer".to_string()
                        }
                    };

                    egui::ComboBox::from_id_salt("uv_viewer_source")
                        .selected_text(&source_name)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected_source, 0, "Composed Result");
                            for idx in 0..self.painter.layers.len() {
                                ui.selectable_value(
                                    &mut selected_source,
                                    idx + 1,
                                    format!("Layer: {}", self.painter.layers[idx].name),
                                );
                            }
                        });
                    if selected_source != current_source {
                        pending_ui_actions.push(ecs::events::UiActionEvent::SetUvViewerSource(
                            selected_source,
                        ));
                    }

                    ui.add_space(15.0);
                    let mut wireframe = self.app_state.ui().show_uv_wireframe;
                    if ui.checkbox(&mut wireframe, "Show UV Wireframe").changed() {
                        pending_ui_actions
                            .push(ecs::events::UiActionEvent::SetUvWireframe(wireframe));
                    }

                    ui.add_space(20.0);
                    ui.label("Zoom:");
                    let mut uv_size = self.app_state.ui().uv_viewer_size;
                    if ui
                        .add(egui::Slider::new(&mut uv_size, 64.0..=512.0).suffix("px"))
                        .changed()
                    {
                        pending_ui_actions
                            .push(ecs::events::UiActionEvent::SetUvViewerSize(uv_size));
                    }
                });

                ui.separator();

                egui::ScrollArea::both().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for tile_idx in 0..num_tiles {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "UDIM Tile {} (U: {}..{})",
                                        tile_idx,
                                        tile_idx,
                                        tile_idx + 1
                                    ))
                                    .strong(),
                                );

                                let tex_id = if self.app_state.ui().uv_viewer_source == 0 {
                                    if tile_idx < viewer.composite_tex_ids.len() {
                                        Some(viewer.composite_tex_ids[tile_idx])
                                    } else {
                                        None
                                    }
                                } else {
                                    let layer_idx = self.app_state.ui().uv_viewer_source - 1;
                                    let global_view_idx =
                                        layer_idx * crate::painter::MAX_UDIMS + tile_idx;
                                    if global_view_idx < viewer.layer_tex_ids.len() {
                                        Some(viewer.layer_tex_ids[global_view_idx])
                                    } else {
                                        None
                                    }
                                };

                                if let Some(id) = tex_id {
                                    let img_size = self.app_state.ui().uv_viewer_size;
                                    let response = ui.image((id, egui::vec2(img_size, img_size)));
                                    let rect = response.rect;

                                    if show_uv_wireframe {
                                        let stroke = egui::Stroke::new(
                                            1.0,
                                            egui::Color32::from_rgba_unmultiplied(
                                                255, 255, 255, 120,
                                            ),
                                        ); // semi-transparent white
                                        for (node, _) in &active_nodes {
                                            if let Some(ref mesh) = node.mesh {
                                                for primitive in &mesh.primitives {
                                                    for chunk in primitive.indices.chunks_exact(3) {
                                                        let i0 = chunk[0] as usize;
                                                        let i1 = chunk[1] as usize;
                                                        let i2 = chunk[2] as usize;

                                                        if i0 < primitive.vertices.len()
                                                            && i1 < primitive.vertices.len()
                                                            && i2 < primitive.vertices.len()
                                                        {
                                                            let uv0 =
                                                                primitive.vertices[i0].tex_coords;
                                                            let uv1 =
                                                                primitive.vertices[i1].tex_coords;
                                                            let uv2 =
                                                                primitive.vertices[i2].tex_coords;

                                                            let local_u0 = uv0[0] - tile_idx as f32;
                                                            let local_u1 = uv1[0] - tile_idx as f32;
                                                            let local_u2 = uv2[0] - tile_idx as f32;

                                                            let in_tile = (local_u0 >= 0.0
                                                                && local_u0 <= 1.0)
                                                                || (local_u1 >= 0.0
                                                                    && local_u1 <= 1.0)
                                                                || (local_u2 >= 0.0
                                                                    && local_u2 <= 1.0);

                                                            if in_tile {
                                                                let p0 = rect.min
                                                                    + egui::vec2(
                                                                        local_u0.clamp(0.0, 1.0)
                                                                            * rect.width(),
                                                                        (1.0 - uv0[1]
                                                                            .clamp(0.0, 1.0))
                                                                            * rect.height(),
                                                                    );
                                                                let p1 = rect.min
                                                                    + egui::vec2(
                                                                        local_u1.clamp(0.0, 1.0)
                                                                            * rect.width(),
                                                                        (1.0 - uv1[1]
                                                                            .clamp(0.0, 1.0))
                                                                            * rect.height(),
                                                                    );
                                                                let p2 = rect.min
                                                                    + egui::vec2(
                                                                        local_u2.clamp(0.0, 1.0)
                                                                            * rect.width(),
                                                                        (1.0 - uv2[1]
                                                                            .clamp(0.0, 1.0))
                                                                            * rect.height(),
                                                                    );

                                                                ui.painter()
                                                                    .line_segment([p0, p1], stroke);
                                                                ui.painter()
                                                                    .line_segment([p1, p2], stroke);
                                                                ui.painter()
                                                                    .line_segment([p2, p0], stroke);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    ui.label("No texture view");
                                }
                            });
                            if tile_idx + 1 < num_tiles {
                                ui.add_space(10.0);
                            }
                        }
                    });
                });
            });

            let egui_output = viewer.egui_ctx.end_pass();
            let paint_jobs = viewer
                .egui_ctx
                .tessellate(egui_output.shapes, egui_output.pixels_per_point);

            for (id, image_delta) in &egui_output.textures_delta.set {
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

            for id in &egui_output.textures_delta.free {
                viewer.egui_renderer.free_texture(id);
            }
        }

        for action in pending_ui_actions {
            self.emit_ui_action(action);
        }
        self.process_ecs_step();

        self.ecs_runtime.ui_frame_ops.begin_uv_egui_frame = false;
        self.ecs_runtime.ui_frame_ops.draw_uv_egui_panels = false;
        self.ecs_runtime.ui_frame_ops.end_uv_egui_frame_and_upload = false;
        self.uv_ui.frame_begun = false;

        Ok(())
    }
}
