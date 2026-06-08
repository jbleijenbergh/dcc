use super::gltf_loader::MaterialInfo;
use super::uv_projector::{BoxProjector, ImportSettings, SeamsOption};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coords: [f32; 2],
}

impl Vertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeUniform {
    pub model_matrix: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
}

pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub material_index: Option<usize>,
    pub bounds_min: glam::Vec3,
    pub bounds_max: glam::Vec3,
}

impl Primitive {
    pub fn new(
        device: &wgpu::Device,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        label: &str,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let num_indices = indices.len() as u32;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Primitive Vertex Buffer", label)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Primitive Index Buffer", label)),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut bounds_min = glam::Vec3::splat(f32::MAX);
        let mut bounds_max = glam::Vec3::splat(f32::MIN);
        for v in &vertices {
            let p = glam::Vec3::from(v.position);
            bounds_min = bounds_min.min(p);
            bounds_max = bounds_max.max(p);
        }

        Self {
            vertices,
            indices,
            vertex_buffer,
            index_buffer,
            num_indices,
            material_index: None,
            bounds_min,
            bounds_max,
        }
    }

    pub fn update_buffers(
        &mut self,
        device: &wgpu::Device,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
    ) {
        use wgpu::util::DeviceExt;
        self.vertices = vertices;
        self.indices = indices;
        self.num_indices = self.indices.len() as u32;
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Primitive Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Primitive Index Buffer"),
            contents: bytemuck::cast_slice(&self.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut bounds_min = glam::Vec3::splat(f32::MAX);
        let mut bounds_max = glam::Vec3::splat(f32::MIN);
        for v in &self.vertices {
            let p = glam::Vec3::from(v.position);
            bounds_min = bounds_min.min(p);
            bounds_max = bounds_max.max(p);
        }
        self.bounds_min = bounds_min;
        self.bounds_max = bounds_max;
    }
}

pub struct Mesh {
    pub primitives: Vec<Primitive>,
}

pub struct Node {
    pub name: Option<String>,
    pub translation: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
    pub mesh: Option<Mesh>,
    pub children: Vec<usize>,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl Node {
    pub fn local_transform(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn update_gpu_transform(&self, queue: &wgpu::Queue, world_matrix: glam::Mat4) {
        let normal_matrix = world_matrix.inverse().transpose();
        let uniform = NodeUniform {
            model_matrix: world_matrix.to_cols_array_2d(),
            normal_matrix: normal_matrix.to_cols_array_2d(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

pub struct Scene {
    pub name: Option<String>,
    pub root_nodes: Vec<usize>,
}

pub struct Document {
    pub scenes: Vec<Scene>,
    pub nodes: Vec<Node>,
    pub active_scene_idx: usize,
    pub materials: Vec<MaterialInfo>,
    pub num_udim_tiles: u32,
}

impl Document {
    pub fn update_num_udim_tiles(&mut self) {
        let mut max_u: f32 = 0.0;
        let nodes = self.get_active_nodes();
        for (node, _) in &nodes {
            if let Some(ref mesh) = node.mesh {
                for primitive in &mesh.primitives {
                    for vertex in &primitive.vertices {
                        max_u = max_u.max(vertex.tex_coords[0]);
                    }
                }
            }
        }
        self.num_udim_tiles = if max_u <= 0.0 {
            1
        } else {
            (max_u.ceil() as u32).clamp(1, 4)
        };
    }

    pub fn recompute_uvs(&mut self, settings: &ImportSettings, device: &wgpu::Device) {
        for node in &mut self.nodes {
            if let Some(ref mut mesh) = node.mesh {
                for primitive in &mut mesh.primitives {
                    if let SeamsOption::GenerateMissing = settings.seams_option {
                        let mut has_uv = false;
                        for v in &primitive.vertices {
                            if v.tex_coords[0].abs() > 1e-4 || v.tex_coords[1].abs() > 1e-4 {
                                has_uv = true;
                                break;
                            }
                        }
                        if has_uv {
                            continue;
                        }
                    }

                    let (new_vertices, new_indices) =
                        BoxProjector::project(&primitive.vertices, &primitive.indices, settings);
                    primitive.update_buffers(device, new_vertices, new_indices);
                }
            }
        }
        self.update_num_udim_tiles();
    }

    pub fn from_single_primitive(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        label: &str,
    ) -> Self {
        let primitive = Primitive::new(device, vertices, indices, label);
        let mesh = Mesh {
            primitives: vec![primitive],
        };

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{} Node Uniform Buffer", label)),
            size: 128, // 2 * 64 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some(&format!("{} Node Bind Group", label)),
        });

        let node = Node {
            name: Some(label.to_string()),
            translation: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
            mesh: Some(mesh),
            children: vec![],
            uniform_buffer,
            bind_group,
        };

        let scene = Scene {
            name: Some("Default Scene".to_string()),
            root_nodes: vec![0],
        };

        let mut doc = Self {
            scenes: vec![scene],
            nodes: vec![node],
            active_scene_idx: 0,
            materials: vec![],
            num_udim_tiles: 1,
        };
        doc.update_num_udim_tiles();
        doc
    }

    pub fn get_active_nodes(&self) -> Vec<(&Node, glam::Mat4)> {
        let mut list = Vec::new();
        if self.scenes.is_empty() {
            return list;
        }
        let scene = &self.scenes[self.active_scene_idx];
        for &root_idx in &scene.root_nodes {
            self.collect_nodes_recursive(root_idx, glam::Mat4::IDENTITY, &mut list);
        }
        list
    }

    fn collect_nodes_recursive<'a>(
        &'a self,
        node_idx: usize,
        parent_world: glam::Mat4,
        list: &mut Vec<(&'a Node, glam::Mat4)>,
    ) {
        if node_idx >= self.nodes.len() {
            return;
        }
        let node = &self.nodes[node_idx];
        let world = parent_world * node.local_transform();
        list.push((node, world));
        for &child_idx in &node.children {
            self.collect_nodes_recursive(child_idx, world, list);
        }
    }

    pub fn compute_bounds(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let mut min = glam::Vec3::splat(f32::MAX);
        let mut max = glam::Vec3::splat(f32::MIN);
        let mut has_vertices = false;

        let nodes = self.get_active_nodes();
        for (node, world_matrix) in &nodes {
            if let Some(ref mesh) = node.mesh {
                for primitive in &mesh.primitives {
                    for vertex in &primitive.vertices {
                        let p = world_matrix.transform_point3(glam::Vec3::from(vertex.position));
                        min = min.min(p);
                        max = max.max(p);
                        has_vertices = true;
                    }
                }
            }
        }

        if has_vertices {
            Some((min, max))
        } else {
            None
        }
    }
}

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
        [-half, -half, half],
        [half, -half, half],
        [half, half, half],
        [-half, half, half],
        [-half, -half, -half],
        [-half, half, -half],
        [half, half, -half],
        [half, -half, -half],
        [-half, half, -half],
        [-half, half, half],
        [half, half, half],
        [half, half, -half],
        [-half, -half, -half],
        [half, -half, -half],
        [half, -half, half],
        [-half, -half, half],
        [half, -half, -half],
        [half, half, -half],
        [half, half, half],
        [half, -half, half],
        [-half, -half, -half],
        [-half, -half, half],
        [-half, half, half],
        [-half, half, -half],
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
            2 => (0.25, 0.50, 0.0, 1.0 / 3.0),
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

    let indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];

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

    mod bounds_tests {
        use super::*;

        #[test]
        fn test_empty_document_bounds_none() {
            let doc = Document {
                scenes: vec![],
                nodes: vec![],
                active_scene_idx: 0,
                materials: vec![],
                num_udim_tiles: 1,
            };
            assert_eq!(doc.compute_bounds(), None);
        }
    }
}
