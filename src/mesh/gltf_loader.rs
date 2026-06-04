use super::geometry::{Vertex, Primitive, Mesh, Node, Scene, Document};

#[derive(Clone, Debug)]
pub struct MaterialInfo {
    pub name: Option<String>,
    pub base_color_factor: [f32; 4], // RGBA
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub alpha_mode: String,          // "Opaque", "Mask", "Blend"
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub has_base_color_texture: bool,
    pub has_normal_texture: bool,
    pub has_metallic_roughness_texture: bool,
    pub gltf_index: Option<usize>,

    // Core properties additional
    pub normal_scale: f32,
    pub has_occlusion_texture: bool,
    pub occlusion_strength: f32,

    // Unlit, IOR, Emissive Strength
    pub unlit: bool,
    pub ior: f32,
    pub emissive_strength: f32,

    // Transmission
    pub transmission_factor: f32,
    pub has_transmission_texture: bool,

    // Volume
    pub thickness_factor: f32,
    pub has_thickness_texture: bool,
    pub attenuation_distance: f32,
    pub attenuation_color: [f32; 3],

    // Specular
    pub specular_factor: f32,
    pub has_specular_texture: bool,
    pub specular_color_factor: [f32; 3],
    pub has_specular_color_texture: bool,

    // Clearcoat
    pub clearcoat_factor: f32,
    pub has_clearcoat_texture: bool,
    pub clearcoat_roughness_factor: f32,
    pub has_clearcoat_roughness_texture: bool,
    pub has_clearcoat_normal_texture: bool,

    // Sheen
    pub sheen_color_factor: [f32; 3],
    pub has_sheen_color_texture: bool,
    pub sheen_roughness_factor: f32,
    pub has_sheen_roughness_texture: bool,

    // Anisotropy
    pub anisotropy_strength: f32,
    pub anisotropy_rotation: f32,
    pub has_anisotropy_texture: bool,

    // Iridescence
    pub iridescence_factor: f32,
    pub has_iridescence_texture: bool,
    pub iridescence_ior: f32,
    pub iridescence_thickness_min: f32,
    pub iridescence_thickness_max: f32,
    pub has_iridescence_thickness_texture: bool,
}

fn parse_gltf_json(path: &std::path::Path) -> Option<serde_json::Value> {
    let file = std::fs::File::open(path).ok()?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file).ok()? };
    
    // Check if it's GLB
    if mmap.len() >= 20 && &mmap[0..4] == b"glTF" {
        let chunk_length = u32::from_le_bytes([mmap[12], mmap[13], mmap[14], mmap[15]]) as usize;
        if mmap.len() >= 20 + chunk_length {
            let json_slice = &mmap[20..20 + chunk_length];
            return serde_json::from_slice(json_slice).ok();
        }
    } else {
        // Standard glTF JSON
        return serde_json::from_slice(&mmap).ok();
    }
    None
}

fn diagnose_missing_assets(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut missing = Vec::new();
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let json_val = match parse_gltf_json(path) {
        Some(val) => val,
        None => return missing,
    };

    // Check buffers
    if let Some(buffers) = json_val.get("buffers").and_then(|b| b.as_array()) {
        for buf in buffers {
            if let Some(uri) = buf.get("uri").and_then(|u| u.as_str()) {
                if !uri.starts_with("data:") {
                    let buf_path = parent.join(uri);
                    if !buf_path.exists() {
                        missing.push(buf_path);
                    }
                }
            }
        }
    }

    // Check images
    if let Some(images) = json_val.get("images").and_then(|i| i.as_array()) {
        for img in images {
            if let Some(uri) = img.get("uri").and_then(|u| u.as_str()) {
                if !uri.starts_with("data:") {
                    let img_path = parent.join(uri);
                    if !img_path.exists() {
                        missing.push(img_path);
                    }
                }
            }
        }
    }

    missing
}

pub fn load_gltf(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    path: &std::path::Path,
) -> Result<Document, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open glTF/GLB file: {}", e))?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }
        .map_err(|e| format!("Failed to memory map file: {}", e))?;
    let (doc, buffers, _) = match gltf::import_slice(&mmap) {
        Ok(data) => data,
        Err(_) => {
            // Fall back to standard import to handle external references
            gltf::import(path)
                .map_err(|e| {
                    let missing = diagnose_missing_assets(path);
                    if !missing.is_empty() {
                        let missing_strs: Vec<String> = missing.iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        format!(
                            "Failed to import glTF: {}. Missing referenced file(s): {}",
                            e,
                            missing_strs.join(", ")
                        )
                    } else {
                        format!("Failed to import glTF: {}", e)
                    }
                })?
        }
    };

    let json_val = parse_gltf_json(path);

    let mut materials = Vec::new();
    for mat in doc.materials() {
        let pbr = mat.pbr_metallic_roughness();
        let ext_json = mat.index().and_then(|idx| {
            json_val.as_ref()
                .and_then(|val| val.get("materials"))
                .and_then(|mats| mats.as_array())
                .and_then(|mats| mats.get(idx))
                .and_then(|mat_val| mat_val.get("extensions"))
        });

        // 1. Core normal scale / occlusion
        let normal_scale = mat.normal_texture().map_or(1.0, |t| t.scale());
        let has_occlusion_texture = mat.occlusion_texture().is_some();
        let occlusion_strength = mat.occlusion_texture().map_or(1.0, |t| t.strength());

        // 2. Unlit
        let unlit = ext_json.and_then(|ext| ext.get("KHR_materials_unlit")).is_some();

        // 3. IOR
        let ior = ext_json.and_then(|ext| ext.get("KHR_materials_ior"))
            .and_then(|ior_val| ior_val.get("ior"))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(1.5);

        // 4. Emissive Strength
        let emissive_strength = ext_json.and_then(|ext| ext.get("KHR_materials_emissive_strength"))
            .and_then(|es_val| es_val.get("emissiveStrength"))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(1.0);

        // 5. Transmission
        let (transmission_factor, has_transmission_texture) = if let Some(trans) = ext_json.and_then(|ext| ext.get("KHR_materials_transmission")) {
            let factor = trans.get("transmissionFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_tex = trans.get("transmissionTexture").is_some();
            (factor, has_tex)
        } else {
            (0.0, false)
        };

        // 6. Volume
        let (thickness_factor, has_thickness_texture, attenuation_distance, attenuation_color) = if let Some(vol) = ext_json.and_then(|ext| ext.get("KHR_materials_volume")) {
            let thick = vol.get("thicknessFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_tex = vol.get("thicknessTexture").is_some();
            let dist = vol.get("attenuationDistance").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(std::f32::INFINITY);
            let color = vol.get("attenuationColor")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    if arr.len() == 3 {
                        Some([
                            arr[0].as_f64()? as f32,
                            arr[1].as_f64()? as f32,
                            arr[2].as_f64()? as f32,
                        ])
                    } else {
                        None
                    }
                })
                .unwrap_or([1.0, 1.0, 1.0]);
            (thick, has_tex, dist, color)
        } else {
            (0.0, false, std::f32::INFINITY, [1.0, 1.0, 1.0])
        };

        // 7. Specular
        let (specular_factor, has_specular_texture, specular_color_factor, has_specular_color_texture) = if let Some(spec) = ext_json.and_then(|ext| ext.get("KHR_materials_specular")) {
            let factor = spec.get("specularFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(1.0);
            let has_spec_tex = spec.get("specularTexture").is_some();
            let color = spec.get("specularColorFactor")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    if arr.len() == 3 {
                        Some([
                            arr[0].as_f64()? as f32,
                            arr[1].as_f64()? as f32,
                            arr[2].as_f64()? as f32,
                        ])
                    } else {
                        None
                    }
                })
                .unwrap_or([1.0, 1.0, 1.0]);
            let has_col_tex = spec.get("specularColorTexture").is_some();
            (factor, has_spec_tex, color, has_col_tex)
        } else {
            (1.0, false, [1.0, 1.0, 1.0], false)
        };

        // 8. Clearcoat
        let (clearcoat_factor, has_clearcoat_texture, clearcoat_roughness_factor, has_clearcoat_roughness_texture, has_clearcoat_normal_texture) = if let Some(cc) = ext_json.and_then(|ext| ext.get("KHR_materials_clearcoat")) {
            let f = cc.get("clearcoatFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_tex = cc.get("clearcoatTexture").is_some();
            let r = cc.get("clearcoatRoughnessFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_r_tex = cc.get("clearcoatRoughnessTexture").is_some();
            let has_n_tex = cc.get("clearcoatNormalTexture").is_some();
            (f, has_tex, r, has_r_tex, has_n_tex)
        } else {
            (0.0, false, 0.0, false, false)
        };

        // 9. Sheen
        let (sheen_color_factor, has_sheen_color_texture, sheen_roughness_factor, has_sheen_roughness_texture) = if let Some(sh) = ext_json.and_then(|ext| ext.get("KHR_materials_sheen")) {
            let color = sh.get("sheenColorFactor")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    if arr.len() == 3 {
                        Some([
                            arr[0].as_f64()? as f32,
                            arr[1].as_f64()? as f32,
                            arr[2].as_f64()? as f32,
                        ])
                    } else {
                        None
                    }
                })
                .unwrap_or([0.0, 0.0, 0.0]);
            let has_col_tex = sh.get("sheenColorTexture").is_some();
            let r = sh.get("sheenRoughnessFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_r_tex = sh.get("sheenRoughnessTexture").is_some();
            (color, has_col_tex, r, has_r_tex)
        } else {
            ([0.0, 0.0, 0.0], false, 0.0, false)
        };

        // 10. Anisotropy
        let (anisotropy_strength, anisotropy_rotation, has_anisotropy_texture) = if let Some(an) = ext_json.and_then(|ext| ext.get("KHR_materials_anisotropy")) {
            let s = an.get("anisotropyStrength").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let r = an.get("anisotropyRotation").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_tex = an.get("anisotropyTexture").is_some();
            (s, r, has_tex)
        } else {
            (0.0, 0.0, false)
        };

        // 11. Iridescence
        let (iridescence_factor, has_iridescence_texture, iridescence_ior, iridescence_thickness_min, iridescence_thickness_max, has_iridescence_thickness_texture) = if let Some(ir) = ext_json.and_then(|ext| ext.get("KHR_materials_iridescence")) {
            let f = ir.get("iridescenceFactor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
            let has_tex = ir.get("iridescenceTexture").is_some();
            let ior = ir.get("iridescenceIor").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(1.3);
            let t_min = ir.get("iridescenceThicknessMin").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(100.0);
            let t_max = ir.get("iridescenceThicknessMax").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(400.0);
            let has_t_tex = ir.get("iridescenceThicknessTexture").is_some();
            (f, has_tex, ior, t_min, t_max, has_t_tex)
        } else {
            (0.0, false, 1.3, 100.0, 400.0, false)
        };

        materials.push(MaterialInfo {
            name: mat.name().map(|s| s.to_string()),
            base_color_factor: pbr.base_color_factor(),
            metallic_factor: pbr.metallic_factor(),
            roughness_factor: pbr.roughness_factor(),
            emissive_factor: mat.emissive_factor(),
            alpha_mode: match mat.alpha_mode() {
                gltf::material::AlphaMode::Opaque => "Opaque".to_string(),
                gltf::material::AlphaMode::Mask   => "Mask".to_string(),
                gltf::material::AlphaMode::Blend  => "Blend".to_string(),
            },
            alpha_cutoff: mat.alpha_cutoff().unwrap_or(0.5),
            double_sided: mat.double_sided(),
            has_base_color_texture: pbr.base_color_texture().is_some(),
            has_normal_texture: mat.normal_texture().is_some(),
            has_metallic_roughness_texture: pbr.metallic_roughness_texture().is_some(),
            gltf_index: mat.index(),

            normal_scale,
            has_occlusion_texture,
            occlusion_strength,

            unlit,
            ior,
            emissive_strength,

            transmission_factor,
            has_transmission_texture,

            thickness_factor,
            has_thickness_texture,
            attenuation_distance,
            attenuation_color,

            specular_factor,
            has_specular_texture,
            specular_color_factor,
            has_specular_color_texture,

            clearcoat_factor,
            has_clearcoat_texture,
            clearcoat_roughness_factor,
            has_clearcoat_roughness_texture,
            has_clearcoat_normal_texture,

            sheen_color_factor,
            has_sheen_color_texture,
            sheen_roughness_factor,
            has_sheen_roughness_texture,

            anisotropy_strength,
            anisotropy_rotation,
            has_anisotropy_texture,

            iridescence_factor,
            has_iridescence_texture,
            iridescence_ior,
            iridescence_thickness_min,
            iridescence_thickness_max,
            has_iridescence_thickness_texture,
        });
    }

    let mut nodes = Vec::new();

    for gltf_node in doc.nodes() {
        let name = gltf_node.name().map(|s| s.to_string());
        let (translation, rotation, scale) = gltf_node.transform().decomposed();
        let translation = glam::Vec3::from_array(translation);
        let rotation = glam::Quat::from_array(rotation);
        let scale = glam::Vec3::from_array(scale);

        let mesh = if let Some(gltf_mesh) = gltf_node.mesh() {
            let mut primitives = Vec::new();
            for (prim_idx, primitive) in gltf_mesh.primitives().enumerate() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                let positions = reader.read_positions().map(|p| p.collect::<Vec<_>>());
                let norm_vec = reader.read_normals().map(|n| n.collect::<Vec<_>>());
                let uv_vec = reader.read_tex_coords(0).map(|t| t.into_f32().collect::<Vec<_>>());
                
                let indices = if let Some(ind_iter) = reader.read_indices() {
                    ind_iter.into_u32().collect::<Vec<u32>>()
                } else {
                    let pos_len = positions.as_ref().map(|p| p.len()).unwrap_or(0);
                    (0..pos_len as u32).collect()
                };

                let mut vertices = Vec::new();
                if let Some(pos_vec) = positions {
                    let computed_normals = if norm_vec.is_none() {
                        let mut normals = vec![[0.0, 1.0, 0.0]; pos_vec.len()];
                        for chunk in indices.chunks_exact(3) {
                            let i0 = chunk[0] as usize;
                            let i1 = chunk[1] as usize;
                            let i2 = chunk[2] as usize;
                            if i0 < pos_vec.len() && i1 < pos_vec.len() && i2 < pos_vec.len() {
                                let p0 = glam::Vec3::from(pos_vec[i0]);
                                let p1 = glam::Vec3::from(pos_vec[i1]);
                                let p2 = glam::Vec3::from(pos_vec[i2]);
                                let normal = (p1 - p0).cross(p2 - p0).normalize_or_zero().into();
                                normals[i0] = normal;
                                normals[i1] = normal;
                                normals[i2] = normal;
                            }
                        }
                        Some(normals)
                    } else {
                        norm_vec
                    };

                    for (i, &p) in pos_vec.iter().enumerate() {
                        let n = computed_normals.as_ref().map(|ns| ns[i]).unwrap_or([0.0, 1.0, 0.0]);
                        let uv = uv_vec.as_ref().map(|uvs| uvs[i]).unwrap_or([0.0, 0.0]);
                        vertices.push(Vertex {
                            position: p,
                            normal: n,
                            tex_coords: uv,
                        });
                    }
                }

                let prim_label = format!("{}_Mesh_{}_Prim_{}", name.as_deref().unwrap_or("Node"), gltf_mesh.index(), prim_idx);

                let mut prim = Primitive::new(device, vertices, indices, &prim_label);
                prim.material_index = primitive.material().index();
                primitives.push(prim);
            }
            Some(Mesh { primitives })
        } else {
            None
        };

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{} Node Uniform Buffer", name.as_deref().unwrap_or("GLTF"))),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some(&format!("{} Node Bind Group", name.as_deref().unwrap_or("GLTF"))),
        });

        let children = gltf_node.children().map(|c| c.index()).collect();

        nodes.push(Node {
            name,
            translation,
            rotation,
            scale,
            mesh,
            children,
            uniform_buffer,
            bind_group,
        });
    }

    let mut scenes = Vec::new();
    for gltf_scene in doc.scenes() {
        let name = gltf_scene.name().map(|s| s.to_string());
        let root_nodes = gltf_scene.nodes().map(|n| n.index()).collect();
        scenes.push(Scene { name, root_nodes });
    }

    let active_scene_idx = doc.default_scene().map(|s| s.index()).unwrap_or(0);

    let mut doc = Document {
        scenes,
        nodes,
        active_scene_idx,
        materials,
        num_udim_tiles: 1,
    };
    doc.update_num_udim_tiles();
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parser_tests {
        use super::*;
        use std::path::Path;

        #[test]
        fn test_parse_gltf_json_iridescence() {
            let path = Path::new("models/IridescenceDielectricSpheres.gltf");
            if !path.exists() {
                return;
            }
            let json_val = parse_gltf_json(path).expect("Failed to parse JSON");
            
            let materials = json_val.get("materials")
                .and_then(|m| m.as_array())
                .expect("No materials array");
                
            let mut found_iridescence = false;
            for mat in materials {
                if let Some(extensions) = mat.get("extensions") {
                    if extensions.get("KHR_materials_iridescence").is_some() {
                        found_iridescence = true;
                        break;
                    }
                }
            }
            assert!(found_iridescence, "Should have found KHR_materials_iridescence extension");
        }

        #[test]
        fn test_parse_gltf_json_volume_transmission() {
            let path = Path::new("models/DragonAttenuation.gltf");
            if !path.exists() {
                return;
            }
            let json_val = parse_gltf_json(path).expect("Failed to parse JSON");
            
            let materials = json_val.get("materials")
                .and_then(|m| m.as_array())
                .expect("No materials array");
                
            let mut found_volume = false;
            let mut found_transmission = false;
            for mat in materials {
                if let Some(extensions) = mat.get("extensions") {
                    if extensions.get("KHR_materials_volume").is_some() {
                        found_volume = true;
                    }
                    if extensions.get("KHR_materials_transmission").is_some() {
                        found_transmission = true;
                    }
                }
            }
            assert!(found_volume, "Should have found KHR_materials_volume extension");
            assert!(found_transmission, "Should have found KHR_materials_transmission extension");
        }

        #[test]
        fn test_diagnose_missing_assets() {
            let temp_dir = std::env::temp_dir();
            let gltf_path = temp_dir.join("temp_test_model.gltf");
            
            let gltf_content = r#"{
                "asset": {
                    "version": "2.0"
                },
                "buffers": [
                    {
                        "uri": "non_existent_buffer.bin",
                        "byteLength": 1024
                    }
                ],
                "images": [
                    {
                        "uri": "non_existent_image.png"
                    }
                ]
            }"#;
            
            std::fs::write(&gltf_path, gltf_content).unwrap();
            
            let missing = diagnose_missing_assets(&gltf_path);
            let _ = std::fs::remove_file(&gltf_path);
            
            assert_eq!(missing.len(), 2);
            let missing_names: Vec<String> = missing.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                .collect();
            assert!(missing_names.contains(&"non_existent_buffer.bin".to_string()));
            assert!(missing_names.contains(&"non_existent_image.png".to_string()));
        }
    }
}

