use super::geometry::{Vertex, Document};

pub fn create_sphere(radius: f32, rings: u32, sectors: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let r_step = 1.0 / (rings as f32 - 1.0);
    let s_step = 1.0 / (sectors as f32 - 1.0);

    for r in 0..rings {
        let latitude = std::f32::consts::PI * (r as f32 * r_step - 0.5);
        let y = latitude.sin();
        let r_sin = latitude.cos();

        for s in 0..sectors {
            let longitude = 2.0 * std::f32::consts::PI * s as f32 * s_step;
            let x = longitude.cos() * r_sin;
            let z = longitude.sin() * r_sin;

            let px = x * radius;
            let py = y * radius;
            let pz = z * radius;

            let nx = x;
            let ny = y;
            let nz = z;

            let u = s as f32 * s_step;
            let v = 1.0 - (r as f32 * r_step);

            vertices.push(Vertex {
                position: [px, py, pz],
                normal: [nx, ny, nz],
                tex_coords: [u, v],
            });
        }
    }

    for r in 0..rings - 1 {
        for s in 0..sectors - 1 {
            let i0 = r * sectors + s;
            let i1 = r * sectors + (s + 1);
            let i2 = (r + 1) * sectors + (s + 1);
            let i3 = (r + 1) * sectors + s;

            indices.push(i0);
            indices.push(i2);
            indices.push(i1);

            indices.push(i0);
            indices.push(i3);
            indices.push(i2);
        }
    }

    (vertices, indices)
}

pub fn create_cube(size: f32) -> (Vec<Vertex>, Vec<u32>) {
    let half = size / 2.0;

    let positions = [
        [-half, -half,  half], [ half, -half,  half], [ half,  half,  half], [-half,  half,  half],
        [-half, -half, -half], [-half,  half, -half], [ half,  half, -half], [ half, -half, -half],
        [-half,  half, -half], [-half,  half,  half], [ half,  half,  half], [ half,  half, -half],
        [-half, -half, -half], [ half, -half, -half], [ half, -half,  half], [-half, -half,  half],
        [ half, -half, -half], [ half,  half, -half], [ half,  half,  half], [ half, -half,  half],
        [-half, -half, -half], [-half, -half,  half], [-half,  half,  half], [-half,  half, -half],
    ];

    let normals = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
    ];

    let face_uvs = [
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        [[1.0, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 1.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        [[1.0, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 1.0]],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for face in 0..6 {
        let (u_min, u_max, v_min, v_max) = match face {
            0 => (0.25, 0.50, 1.0 / 3.0, 2.0 / 3.0),
            1 => (0.75, 1.00, 1.0 / 3.0, 2.0 / 3.0),
            2 => (0.25, 0.50, 0.0,       1.0 / 3.0),
            3 => (0.25, 0.50, 2.0 / 3.0, 1.0),
            4 => (0.50, 0.75, 1.0 / 3.0, 2.0 / 3.0),
            5 => (0.00, 0.25, 1.0 / 3.0, 2.0 / 3.0),
            _ => unreachable!(),
        };

        for v in 0..4 {
            let original_uv = face_uvs[face][v];
            let u = u_min + original_uv[0] * (u_max - u_min);
            let v_coord = v_min + original_uv[1] * (v_max - v_min);

            vertices.push(Vertex {
                position: positions[face * 4 + v],
                normal: normals[face],
                tex_coords: [u, v_coord],
            });
        }

        let base = (face * 4) as u32;
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);
    }

    (vertices, indices)
}

pub fn create_plane(size: f32) -> (Vec<Vertex>, Vec<u32>) {
    let half = size / 2.0;

    let vertices = vec![
        Vertex {
            position: [-half, -half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 1.0],
        },
        Vertex {
            position: [half, -half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 1.0],
        },
        Vertex {
            position: [half, half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 0.0],
        },
        Vertex {
            position: [-half, half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 0.0],
        },

        Vertex {
            position: [half, -half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [1.0, 1.0],
        },
        Vertex {
            position: [-half, -half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [0.0, 1.0],
        },
        Vertex {
            position: [-half, half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [0.0, 0.0],
        },
        Vertex {
            position: [half, half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [1.0, 0.0],
        },
    ];

    let indices = vec![
        0, 1, 2,
        0, 2, 3,

        4, 5, 6,
        4, 6, 7,
    ];

    (vertices, indices)
}

pub fn create_sphere_document(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    radius: f32,
    rings: u32,
    sectors: u32,
) -> Document {
    let (vertices, indices) = create_sphere(radius, rings, sectors);
    Document::from_single_primitive(device, layout, vertices, indices, "Sphere")
}

pub fn create_cube_document(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: f32,
) -> Document {
    let (vertices, indices) = create_cube(size);
    Document::from_single_primitive(device, layout, vertices, indices, "Cube")
}

pub fn create_plane_document(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: f32,
) -> Document {
    let (vertices, indices) = create_plane(size);
    Document::from_single_primitive(device, layout, vertices, indices, "Plane")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sphere_tests {
        use super::*;

        #[test]
        fn test_sphere_generation_counts() {
            let (vertices, indices) = create_sphere(1.0, 10, 10);
            assert_eq!(vertices.len(), 100);
            assert_eq!(indices.len(), 486);
        }

        #[test]
        fn test_sphere_generation_bounds() {
            let radius = 1.5;
            let (vertices, _indices) = create_sphere(radius, 10, 10);
            for v in vertices {
                let len = glam::Vec3::from(v.position).length();
                assert!(
                    (len - radius).abs() < 1e-4,
                    "Vertex position length {} should be close to radius {}",
                    len,
                    radius
                );
            }
        }

        #[test]
        fn test_sphere_seam_alignment() {
            let rings = 32;
            let sectors = 32;
            let (vertices, _indices) = create_sphere(1.5, rings, sectors);
            for r in 0..rings {
                let idx_start = (r * sectors) as usize;
                let idx_end = (r * sectors + sectors - 1) as usize;
                let v_start = &vertices[idx_start];
                let v_end = &vertices[idx_end];

                // Compare positions
                let p_start = glam::Vec3::from(v_start.position);
                let p_end = glam::Vec3::from(v_end.position);
                assert!(
                    (p_start - p_end).length() < 1e-5,
                    "Ring {}: position mismatch {:?} vs {:?}",
                    r,
                    p_start,
                    p_end
                );

                // Compare texture coordinates
                assert!(
                    (v_start.tex_coords[0] - 0.0).abs() < 1e-5,
                    "Ring {}: U start should be 0.0, got {}",
                    r,
                    v_start.tex_coords[0]
                );
                assert!(
                    (v_end.tex_coords[0] - 1.0).abs() < 1e-5,
                    "Ring {}: U end should be 1.0, got {}",
                    r,
                    v_end.tex_coords[0]
                );
                assert!(
                    (v_start.tex_coords[1] - v_end.tex_coords[1]).abs() < 1e-5,
                    "Ring {}: V mismatch {} vs {}",
                    r,
                    v_start.tex_coords[1],
                    v_end.tex_coords[1]
                );
            }
        }
    }

    mod cube_tests {
        use super::*;

        #[test]
        fn test_cube_generation_counts() {
            let (vertices, indices) = create_cube(2.0);
            assert_eq!(vertices.len(), 24);
            assert_eq!(indices.len(), 36);
        }

        #[test]
        fn test_cube_generation_bounds() {
            let size = 2.0;
            let half = size / 2.0;
            let (vertices, _indices) = create_cube(size);
            for v in vertices {
                for i in 0..3 {
                    assert!(v.position[i].abs() <= half + 1e-5);
                }
            }
        }
    }

    mod plane_tests {
        use super::*;

        #[test]
        fn test_plane_generation_counts() {
            let (vertices, indices) = create_plane(5.0);
            assert_eq!(vertices.len(), 8);
            assert_eq!(indices.len(), 12);
        }
    }
}
