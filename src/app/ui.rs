use std::time::Instant;

use super::LoadError;
use crate::mesh::{MaterialInfo, Node, Mesh};

#[derive(Debug, Default, Clone)]
pub(super) struct TransientUiState {
    pub error_details: Option<LoadError>,
    pub error_time: Option<Instant>,
    pub settings_feedback: Option<String>,
}

pub(super) fn draw_node_tree(ui: &mut egui::Ui, nodes: &[Node], node_idx: usize, materials: &[MaterialInfo]) {
    if node_idx >= nodes.len() {
        return;
    }
    ui.push_id(node_idx, |ui| {
        let node = &nodes[node_idx];
        let label = node.name.clone().unwrap_or_else(|| format!("Node {}", node_idx));
        
        let has_children = !node.children.is_empty();
        let has_mesh = node.mesh.is_some();
        
        if !has_children && !has_mesh {
            ui.horizontal(|ui| {
                ui.label(format!("📄 {}", label));
            });
        } else {
            egui::CollapsingHeader::new(format!("📁 {}", label))
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(ref mesh) = node.mesh {
                        draw_mesh_info(ui, mesh, materials);
                    }
                    for &child_idx in &node.children {
                        draw_node_tree(ui, nodes, child_idx, materials);
                    }
                });
        }
    });
}

pub(super) fn draw_mesh_info(ui: &mut egui::Ui, mesh: &Mesh, materials: &[MaterialInfo]) {
    egui::CollapsingHeader::new("📦 Mesh")
        .default_open(false)
        .show(ui, |ui| {
            for (idx, prim) in mesh.primitives.iter().enumerate() {
                ui.push_id(idx, |ui| {
                    egui::CollapsingHeader::new(format!("📐 Primitive {}", idx))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} verts  •  {} tris",
                                        prim.vertices.len(),
                                        prim.num_indices / 3,
                                    ))
                                    .weak()
                                    .size(10.0),
                                );
                            });

                            ui.add_space(2.0);
                            if let Some(mat) = prim.material_index.and_then(|idx| materials.get(idx)) {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("🎨 Material:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(mat.name.as_deref().unwrap_or("Material"))
                                            .strong()
                                            .size(11.0),
                                    );
                                });
                            } else {
                                ui.label(egui::RichText::new("No material").size(10.0).weak().italics());
                            }
                        });
                });
            }
        });
}

pub(super) fn draw_material_details(ui: &mut egui::Ui, idx: usize, mat: &MaterialInfo) {
    ui.push_id(idx, |ui| {
        egui::CollapsingHeader::new(
            egui::RichText::new(format!(
                "🎨  {}",
                mat.name.as_deref().unwrap_or("Material")
            ))
            .strong(),
        )
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("mat_detail_grid")
                .num_columns(2)
                .spacing([8.0, 3.0])
                .striped(true)
                .show(ui, |ui| {
                    let bc = mat.base_color_factor;
                    ui.label(egui::RichText::new("Base Color").size(11.0));
                    ui.horizontal(|ui| {
                        let swatch_color = egui::Color32::from_rgba_unmultiplied(
                            (bc[0] * 255.0) as u8,
                            (bc[1] * 255.0) as u8,
                            (bc[2] * 255.0) as u8,
                            (bc[3] * 255.0) as u8,
                        );
                        egui::color_picker::show_color(ui, swatch_color, egui::Vec2::new(18.0, 14.0));
                        ui.label(
                            egui::RichText::new(format!(
                                "({:.2}, {:.2}, {:.2}, {:.2})",
                                bc[0], bc[1], bc[2], bc[3]
                            ))
                            .size(10.0)
                            .weak(),
                        );
                    });
                    ui.end_row();

                    ui.label(egui::RichText::new("Metallic").size(11.0));
                    ui.add(egui::ProgressBar::new(mat.metallic_factor)
                        .desired_width(80.0)
                        .text(format!("{:.2}", mat.metallic_factor)));
                    ui.end_row();

                    ui.label(egui::RichText::new("Roughness").size(11.0));
                    ui.add(egui::ProgressBar::new(mat.roughness_factor)
                        .desired_width(80.0)
                        .text(format!("{:.2}", mat.roughness_factor)));
                    ui.end_row();

                    if mat.normal_scale != 1.0 {
                        ui.label(egui::RichText::new("Normal Scale").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.normal_scale)).size(10.0).weak());
                        ui.end_row();
                    }

                    if mat.has_occlusion_texture {
                        ui.label(egui::RichText::new("Occlusion Strength").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.occlusion_strength)).size(10.0).weak());
                        ui.end_row();
                    }

                    let em = mat.emissive_factor;
                    let em_mag = (em[0] * em[0] + em[1] * em[1] + em[2] * em[2]).sqrt();
                    if em_mag > 0.001 {
                        ui.label(egui::RichText::new("Emissive").size(11.0));
                        ui.horizontal(|ui| {
                            let em_color = egui::Color32::from_rgb(
                                (em[0].min(1.0) * 255.0) as u8,
                                (em[1].min(1.0) * 255.0) as u8,
                                (em[2].min(1.0) * 255.0) as u8,
                            );
                            egui::color_picker::show_color(ui, em_color, egui::Vec2::new(18.0, 14.0));
                            ui.label(
                                egui::RichText::new(format!(
                                    "({:.2}, {:.2}, {:.2})",
                                    em[0], em[1], em[2]
                                ))
                                .size(10.0)
                                .weak(),
                            );
                        });
                        ui.end_row();
                    }

                    if mat.emissive_strength != 1.0 {
                        ui.label(egui::RichText::new("Emissive Strength").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}x", mat.emissive_strength)).size(10.0).weak());
                        ui.end_row();
                    }

                    if mat.unlit {
                        ui.label(egui::RichText::new("Unlit").size(11.0));
                        ui.label(egui::RichText::new("Yes").size(10.0).color(egui::Color32::from_rgb(255, 215, 0)));
                        ui.end_row();
                    }

                    if (mat.ior - 1.5).abs() > 0.001 {
                        ui.label(egui::RichText::new("Index of Refraction").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.3}", mat.ior)).size(10.0).weak());
                        ui.end_row();
                    }

                    if mat.transmission_factor > 0.001 {
                        ui.label(egui::RichText::new("Transmission").size(11.0));
                        ui.add(egui::ProgressBar::new(mat.transmission_factor)
                            .desired_width(80.0)
                            .text(format!("{:.2}", mat.transmission_factor)));
                        ui.end_row();
                    }

                    if mat.thickness_factor > 0.001 {
                        ui.label(egui::RichText::new("Volume Thickness").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.thickness_factor)).size(10.0).weak());
                        ui.end_row();

                        if mat.attenuation_distance.is_finite() {
                            ui.label(egui::RichText::new("Attenuation Dist").size(11.0));
                            ui.label(egui::RichText::new(format!("{:.2}", mat.attenuation_distance)).size(10.0).weak());
                            ui.end_row();
                        }

                        ui.label(egui::RichText::new("Attenuation Color").size(11.0));
                        ui.horizontal(|ui| {
                            let ac = mat.attenuation_color;
                            let ac_color = egui::Color32::from_rgb(
                                (ac[0] * 255.0) as u8,
                                (ac[1] * 255.0) as u8,
                                (ac[2] * 255.0) as u8,
                            );
                            egui::color_picker::show_color(ui, ac_color, egui::Vec2::new(18.0, 14.0));
                        });
                        ui.end_row();
                    }

                    if (mat.specular_factor - 1.0).abs() > 0.001 || mat.specular_color_factor != [1.0, 1.0, 1.0] {
                        ui.label(egui::RichText::new("Specular Factor").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.specular_factor)).size(10.0).weak());
                        ui.end_row();

                        ui.label(egui::RichText::new("Specular Color").size(11.0));
                        ui.horizontal(|ui| {
                            let sc = mat.specular_color_factor;
                            let sc_color = egui::Color32::from_rgb(
                                (sc[0] * 255.0) as u8,
                                (sc[1] * 255.0) as u8,
                                (sc[2] * 255.0) as u8,
                            );
                            egui::color_picker::show_color(ui, sc_color, egui::Vec2::new(18.0, 14.0));
                        });
                        ui.end_row();
                    }

                    if mat.clearcoat_factor > 0.001 {
                        ui.label(egui::RichText::new("Clearcoat Factor").size(11.0));
                        ui.add(egui::ProgressBar::new(mat.clearcoat_factor)
                            .desired_width(80.0)
                            .text(format!("{:.2}", mat.clearcoat_factor)));
                        ui.end_row();

                        ui.label(egui::RichText::new("Clearcoat Rough").size(11.0));
                        ui.add(egui::ProgressBar::new(mat.clearcoat_roughness_factor)
                            .desired_width(80.0)
                            .text(format!("{:.2}", mat.clearcoat_roughness_factor)));
                        ui.end_row();
                    }

                    if mat.sheen_color_factor != [0.0, 0.0, 0.0] {
                        ui.label(egui::RichText::new("Sheen Color").size(11.0));
                        ui.horizontal(|ui| {
                            let shc = mat.sheen_color_factor;
                            let shc_color = egui::Color32::from_rgb(
                                (shc[0] * 255.0) as u8,
                                (shc[1] * 255.0) as u8,
                                (shc[2] * 255.0) as u8,
                            );
                            egui::color_picker::show_color(ui, shc_color, egui::Vec2::new(18.0, 14.0));
                        });
                        ui.end_row();

                        ui.label(egui::RichText::new("Sheen Roughness").size(11.0));
                        ui.add(egui::ProgressBar::new(mat.sheen_roughness_factor)
                            .desired_width(80.0)
                            .text(format!("{:.2}", mat.sheen_roughness_factor)));
                        ui.end_row();
                    }

                    if mat.anisotropy_strength.abs() > 0.001 {
                        ui.label(egui::RichText::new("Anisotropy Strength").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.anisotropy_strength)).size(10.0).weak());
                        ui.end_row();

                        ui.label(egui::RichText::new("Anisotropy Rotation").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.1}°", mat.anisotropy_rotation.to_degrees())).size(10.0).weak());
                        ui.end_row();
                    }

                    if mat.iridescence_factor > 0.001 {
                        ui.label(egui::RichText::new("Iridescence Factor").size(11.0));
                        ui.add(egui::ProgressBar::new(mat.iridescence_factor)
                            .desired_width(80.0)
                            .text(format!("{:.2}", mat.iridescence_factor)));
                        ui.end_row();

                        ui.label(egui::RichText::new("Iridescence IOR").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.iridescence_ior)).size(10.0).weak());
                        ui.end_row();

                        ui.label(egui::RichText::new("Iridescence Thick").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.0}-{:.0} nm", mat.iridescence_thickness_min, mat.iridescence_thickness_max)).size(10.0).weak());
                        ui.end_row();
                    }

                    ui.label(egui::RichText::new("Alpha Mode").size(11.0));
                    let alpha_color = match mat.alpha_mode.as_str() {
                        "Blend" => egui::Color32::from_rgb(100, 160, 255),
                        "Mask"  => egui::Color32::from_rgb(255, 190, 80),
                        _       => egui::Color32::from_rgb(120, 200, 120),
                    };
                    ui.label(egui::RichText::new(&mat.alpha_mode).size(10.0).color(alpha_color));
                    ui.end_row();

                    if mat.alpha_mode == "Mask" {
                        ui.label(egui::RichText::new("Alpha Cutoff").size(11.0));
                        ui.label(egui::RichText::new(format!("{:.2}", mat.alpha_cutoff)).size(10.0).weak());
                        ui.end_row();
                    }

                    ui.label(egui::RichText::new("Double-Sided").size(11.0));
                    ui.label(
                        egui::RichText::new(if mat.double_sided { "Yes" } else { "No" })
                            .size(10.0)
                            .weak(),
                    );
                    ui.end_row();
                });

            let active_slots: Vec<(&str, bool)> = [
                ("BaseColor", mat.has_base_color_texture),
                ("Normal", mat.has_normal_texture),
                ("MetalRough", mat.has_metallic_roughness_texture),
                ("Occlusion", mat.has_occlusion_texture),
                ("Transmission", mat.has_transmission_texture),
                ("Thickness", mat.has_thickness_texture),
                ("Specular", mat.has_specular_texture),
                ("SpecColor", mat.has_specular_color_texture),
                ("Clearcoat", mat.has_clearcoat_texture),
                ("ClearcoatRough", mat.has_clearcoat_roughness_texture),
                ("ClearcoatNormal", mat.has_clearcoat_normal_texture),
                ("SheenColor", mat.has_sheen_color_texture),
                ("SheenRough", mat.has_sheen_roughness_texture),
                ("Anisotropy", mat.has_anisotropy_texture),
                ("Iridescence", mat.has_iridescence_texture),
                ("IridescenceThick", mat.has_iridescence_thickness_texture),
            ]
            .into_iter()
            .filter(|&(_, has)| has)
            .collect();

            if !active_slots.is_empty() {
                ui.add_space(5.0);
                ui.label(egui::RichText::new("Active Texture Slots").size(10.0).weak());
                ui.horizontal_wrapped(|ui| {
                    let present_color = egui::Color32::from_rgb(80, 190, 110);
                    for (label, _) in active_slots {
                        ui.label(
                            egui::RichText::new(label)
                                .size(9.5)
                                .color(present_color)
                                .background_color(egui::Color32::from_black_alpha(60)),
                        );
                    }
                });
            }
        });
    });
}
