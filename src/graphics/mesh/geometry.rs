use super::loader::MaterialInfo;
use super::uv::{BoxProjector, ImportSettings, SeamsOption};

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

#[derive(Clone)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub vertex_buffer: std::sync::Arc<wgpu::Buffer>,
    pub index_buffer: std::sync::Arc<wgpu::Buffer>,
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
        let vertex_buffer = std::sync::Arc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Primitive Vertex Buffer", label)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        let index_buffer = std::sync::Arc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Primitive Index Buffer", label)),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        }));

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
        self.vertex_buffer = std::sync::Arc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Primitive Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        self.index_buffer = std::sync::Arc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Primitive Index Buffer"),
            contents: bytemuck::cast_slice(&self.indices),
            usage: wgpu::BufferUsages::INDEX,
        }));

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

#[derive(Clone)]
pub struct Mesh {
    pub primitives: Vec<Primitive>,
}

#[derive(Clone)]
pub struct Node {
    pub name: Option<String>,
    pub translation: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
    pub mesh: Option<Mesh>,
    pub children: Vec<usize>,
    pub uniform_buffer: std::sync::Arc<wgpu::Buffer>,
    pub bind_group: std::sync::Arc<wgpu::BindGroup>,
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

#[derive(Clone)]
pub struct Scene {
    pub name: Option<String>,
    pub root_nodes: Vec<usize>,
}

#[derive(Clone)]
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

        let uniform_buffer = std::sync::Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{} Node Uniform Buffer", label)),
            size: 128, // 2 * 64 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        let bind_group = std::sync::Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some(&format!("{} Node Bind Group", label)),
        }));

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

    pub fn into_active_nodes(mut self) -> Vec<(Option<String>, glam::Mat4, Mesh, std::sync::Arc<wgpu::Buffer>, std::sync::Arc<wgpu::BindGroup>)> {
        let mut list = Vec::new();
        if self.scenes.is_empty() {
            return list;
        }
        let scene = &self.scenes[self.active_scene_idx];
        let mut world_matrices = vec![glam::Mat4::IDENTITY; self.nodes.len()];
        let mut active_indices = std::collections::HashSet::new();
        for &root_idx in &scene.root_nodes {
            self.collect_indices_and_matrices_recursive(root_idx, glam::Mat4::IDENTITY, &mut world_matrices, &mut active_indices);
        }

        let nodes = std::mem::take(&mut self.nodes);
        for (idx, node) in nodes.into_iter().enumerate() {
            if active_indices.contains(&idx) {
                if let Some(mesh) = node.mesh {
                    let matrix = world_matrices[idx];
                    list.push((node.name, matrix, mesh, node.uniform_buffer, node.bind_group));
                }
            }
        }
        list
    }

    fn collect_indices_and_matrices_recursive(
        &self,
        node_idx: usize,
        parent_world: glam::Mat4,
        world_matrices: &mut [glam::Mat4],
        active_indices: &mut std::collections::HashSet<usize>,
    ) {
        if node_idx >= self.nodes.len() {
            return;
        }
        let node = &self.nodes[node_idx];
        let world = parent_world * node.local_transform();
        world_matrices[node_idx] = world;
        active_indices.insert(node_idx);
        for &child_idx in &node.children {
            self.collect_indices_and_matrices_recursive(child_idx, world, world_matrices, active_indices);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
