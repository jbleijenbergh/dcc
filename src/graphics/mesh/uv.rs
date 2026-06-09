use super::geometry::Vertex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeamsOption {
    GenerateMissing,
    RecomputeAll,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarginSize {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IslandOrientation {
    Unconstrained,
    AlignWith3DMesh,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImportSettings {
    pub seams_option: SeamsOption,
    pub margin_size: MarginSize,
    pub island_orientation: IslandOrientation,
}

pub struct BoxProjector;

impl BoxProjector {
    pub fn project(
        vertices: &[Vertex],
        indices: &[u32],
        settings: &ImportSettings,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut new_vertices = Vec::new();
        let mut new_indices = Vec::new();

        let margin = match settings.margin_size {
            MarginSize::Small => 0.01f32,
            MarginSize::Medium => 0.03f32,
            MarginSize::Large => 0.06f32,
        };

        let num_triangles = indices.len() / 3;
        let mut face_axes = Vec::with_capacity(num_triangles);
        let mut projected_uvs = vec![[0.0f32; 2]; indices.len()];

        let mut u_min = [f32::MAX; 6];
        let mut u_max = [f32::MIN; 6];
        let mut v_min = [f32::MAX; 6];
        let mut v_max = [f32::MIN; 6];

        for t in 0..num_triangles {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;

            let p0 = glam::Vec3::from(vertices[i0].position);
            let p1 = glam::Vec3::from(vertices[i1].position);
            let p2 = glam::Vec3::from(vertices[i2].position);

            let n = (p1 - p0).cross(p2 - p0).normalize_or_zero();

            let abs_n = n.abs();
            let axis = if abs_n.x >= abs_n.y && abs_n.x >= abs_n.z {
                if n.x > 0.0 {
                    0
                } else {
                    1
                }
            } else if abs_n.y >= abs_n.x && abs_n.y >= abs_n.z {
                if n.y > 0.0 {
                    2
                } else {
                    3
                }
            } else {
                if n.z > 0.0 {
                    4
                } else {
                    5
                }
            };

            face_axes.push(axis);

            for (idx_offset, &v_idx) in [i0, i1, i2].iter().enumerate() {
                let p = vertices[v_idx].position;
                let (mut u_raw, mut v_raw) = match axis {
                    0 => (p[2], p[1]),
                    1 => (-p[2], p[1]),
                    2 => (p[0], p[2]),
                    3 => (p[0], -p[2]),
                    4 => (-p[0], p[1]),
                    5 => (p[0], p[1]),
                    _ => unreachable!(),
                };

                if let IslandOrientation::Unconstrained = settings.island_orientation {
                    let angle = 30.0f32.to_radians();
                    let cos_a = angle.cos();
                    let sin_a = angle.sin();
                    let u_rot = u_raw * cos_a - v_raw * sin_a;
                    let v_rot = u_raw * sin_a + v_raw * cos_a;
                    u_raw = u_rot;
                    v_raw = v_rot;
                }

                projected_uvs[t * 3 + idx_offset] = [u_raw, v_raw];

                u_min[axis] = u_min[axis].min(u_raw);
                u_max[axis] = u_max[axis].max(u_raw);
                v_min[axis] = v_min[axis].min(v_raw);
                v_max[axis] = v_max[axis].max(v_raw);
            }
        }

        for t in 0..num_triangles {
            let axis = face_axes[t];
            let tile_idx = axis / 2; // 0, 1, or 2
            let is_right = (axis % 2) == 1;

            let target_u_min = tile_idx as f32 + if is_right { 0.5 + margin } else { margin };
            let target_u_max = tile_idx as f32 + if is_right { 1.0 - margin } else { 0.5 - margin };
            let target_v_min = margin;
            let target_v_max = 1.0 - margin;

            let u_w = u_max[axis] - u_min[axis];
            let v_h = v_max[axis] - v_min[axis];

            for idx_offset in 0..3 {
                let orig_vertex_idx = indices[t * 3 + idx_offset];
                let orig_vertex = &vertices[orig_vertex_idx as usize];

                let raw_uv = projected_uvs[t * 3 + idx_offset];
                let u = raw_uv[0];
                let v = raw_uv[1];

                let mut local_u = if u_w > 1e-5 {
                    (u - u_min[axis]) / u_w
                } else {
                    0.5
                };
                let mut local_v = if v_h > 1e-5 {
                    (v - v_min[axis]) / v_h
                } else {
                    0.5
                };

                if let IslandOrientation::AlignWith3DMesh = settings.island_orientation {
                    let aspect_raw = if v_h > 1e-5 { u_w / v_h } else { 1.0 };
                    let aspect_target =
                        (target_u_max - target_u_min) / (target_v_max - target_v_min);
                    if aspect_raw > aspect_target {
                        let scale = aspect_target / aspect_raw;
                        local_v = 0.5 + (local_v - 0.5) * scale;
                    } else {
                        let scale = aspect_raw / aspect_target;
                        local_u = 0.5 + (local_u - 0.5) * scale;
                    }
                }

                let mapped_u = target_u_min + local_u * (target_u_max - target_u_min);
                let mapped_v = target_v_min + local_v * (target_v_max - target_v_min);

                new_vertices.push(Vertex {
                    position: orig_vertex.position,
                    normal: orig_vertex.normal,
                    tex_coords: [mapped_u, mapped_v],
                });
                new_indices.push((new_vertices.len() - 1) as u32);
            }
        }

        (new_vertices, new_indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod projection_tests {
        use super::*;

        #[test]
        fn test_box_projector_simple_triangle() {
            let vertices = vec![
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
            ];
            let indices = vec![0, 1, 2];
            let settings = ImportSettings {
                seams_option: SeamsOption::RecomputeAll,
                margin_size: MarginSize::Small,
                island_orientation: IslandOrientation::AlignWith3DMesh,
            };

            let (new_vertices, new_indices) = BoxProjector::project(&vertices, &indices, &settings);

            assert_eq!(new_vertices.len(), 3);
            assert_eq!(new_indices.len(), 3);

            for v in new_vertices {
                assert!(
                    v.tex_coords[0] >= 2.0,
                    "U coord {} should be in tile 2",
                    v.tex_coords[0]
                );
                assert!(
                    v.tex_coords[0] <= 2.5,
                    "U coord {} should be in tile 2",
                    v.tex_coords[0]
                );
                assert!(
                    v.tex_coords[1] >= 0.0,
                    "V coord {} should be positive",
                    v.tex_coords[1]
                );
                assert!(
                    v.tex_coords[1] <= 1.0,
                    "V coord {} should be less than 1.0",
                    v.tex_coords[1]
                );
            }
        }
    }

    mod margin_tests {
        use super::*;

        #[test]
        fn test_different_margins() {
            let vertices = vec![
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tex_coords: [0.0, 0.0],
                },
            ];
            let indices = vec![0, 1, 2];

            let margins = [
                (MarginSize::Small, 0.01f32),
                (MarginSize::Medium, 0.03f32),
                (MarginSize::Large, 0.06f32),
            ];

            for (margin_size, margin_val) in margins {
                let settings = ImportSettings {
                    seams_option: SeamsOption::RecomputeAll,
                    margin_size,
                    island_orientation: IslandOrientation::Unconstrained,
                };
                let (new_vertices, _) = BoxProjector::project(&vertices, &indices, &settings);
                let min_u = new_vertices
                    .iter()
                    .map(|v| v.tex_coords[0])
                    .fold(f32::MAX, f32::min);
                let min_v = new_vertices
                    .iter()
                    .map(|v| v.tex_coords[1])
                    .fold(f32::MAX, f32::min);
                assert!(
                    (min_u - (2.0 + margin_val)).abs() < 1e-4,
                    "Min U {} should be close to {}",
                    min_u,
                    2.0 + margin_val
                );
                assert!(
                    (min_v - margin_val).abs() < 1e-4,
                    "Min V {} should be close to {}",
                    min_v,
                    margin_val
                );
            }
        }
    }
}
