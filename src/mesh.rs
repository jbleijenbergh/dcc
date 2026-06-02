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

pub fn create_sphere(radius: f32, rings: u32, sectors: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let r_step = 1.0 / (rings as f32 - 1.0);
    let s_step = 1.0 / (sectors as f32 - 1.0);

    for r in 0..rings {
        // latitude angle from -PI/2 to PI/2
        let latitude = std::f32::consts::PI * (r as f32 * r_step - 0.5);
        let y = latitude.sin();
        let r_sin = latitude.cos();

        for s in 0..sectors {
            // longitude angle from 0 to 2*PI
            let longitude = 2.0 * std::f32::consts::PI * s as f32 * s_step;
            let x = longitude.cos() * r_sin;
            let z = longitude.sin() * r_sin;

            let px = x * radius;
            let py = y * radius;
            let pz = z * radius;

            // For sphere, unit normal is simply the position normalized (which x, y, z is)
            let nx = x;
            let ny = y;
            let nz = z;

            // U increases from left to right on the front face
            let u = s as f32 * s_step;
            // Invert V so V=0 (top of texture) maps to North Pole (y=1) instead of South Pole
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

            // Triangle 1
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);

            // Triangle 2
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
        // Front face (z = +half)
        [-half, -half,  half], [ half, -half,  half], [ half,  half,  half], [-half,  half,  half],
        // Back face (z = -half)
        [-half, -half, -half], [-half,  half, -half], [ half,  half, -half], [ half, -half, -half],
        // Top face (y = +half)
        [-half,  half, -half], [-half,  half,  half], [ half,  half,  half], [ half,  half, -half],
        // Bottom face (y = -half)
        [-half, -half, -half], [ half, -half, -half], [ half, -half,  half], [-half, -half,  half],
        // Right face (x = +half)
        [ half, -half, -half], [ half,  half, -half], [ half,  half,  half], [ half, -half,  half],
        // Left face (x = -half)
        [-half, -half, -half], [-half, -half,  half], [-half,  half,  half], [-half,  half, -half],
    ];

    let normals = [
        [0.0, 0.0, 1.0],   // Front
        [0.0, 0.0, -1.0],  // Back
        [0.0, 1.0, 0.0],   // Top
        [0.0, -1.0, 0.0],  // Bottom
        [1.0, 0.0, 0.0],   // Right
        [-1.0, 0.0, 0.0],  // Left
    ];

    // UVs for each of the 6 faces.
    // Each face has 4 vertices in the following order of positions:
    // Front: [BL, BR, TR, TL] -> Upright layout: [BL, BR, TR, TL]
    // Back: [BR, TR, TL, BL] (looking at it from behind, BL is [half, -half, -half]) -> Upright layout: [BR, TR, TL, BL]
    // Top: [TL, BL, BR, TR] (looking from above, Up is -Z) -> Upright layout: [TL, BL, BR, TR]
    // Bottom: [BL, BR, TR, TL] (looking from below, Up is +Z) -> Upright layout: [BL, BR, TR, TL]
    // Right: [BR, TR, TL, BL] (looking from right, Up is +Y, Left is +Z) -> Upright layout: [BR, TR, TL, BL]
    // Left: [BL, BR, TR, TL] (looking from left, Up is +Y, Left is -Z) -> Upright layout: [BL, BR, TR, TL]
    let face_uvs = [
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], // Front
        [[1.0, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 1.0]], // Back
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], // Top
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], // Bottom
        [[1.0, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 1.0]], // Right
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], // Left
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for face in 0..6 {
        // Define sub-rectangles in the 4x3 grid atlas.
        // Each column is 0.25 wide, each row is 1/3 (0.333333) high.
        let (u_min, u_max, v_min, v_max) = match face {
            0 => (0.25, 0.50, 1.0 / 3.0, 2.0 / 3.0), // Front
            1 => (0.75, 1.00, 1.0 / 3.0, 2.0 / 3.0), // Back
            2 => (0.25, 0.50, 0.0,       1.0 / 3.0), // Top
            3 => (0.25, 0.50, 2.0 / 3.0, 1.0),       // Bottom
            4 => (0.50, 0.75, 1.0 / 3.0, 2.0 / 3.0), // Right
            5 => (0.00, 0.25, 1.0 / 3.0, 2.0 / 3.0), // Left
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

    // Double-sided quad (front facing +Z, back facing -Z)
    let vertices = vec![
        // Front face (facing +Z, CCW winding, normal [0.0, 0.0, 1.0])
        Vertex {
            position: [-half, -half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 1.0], // Bottom-Left
        },
        Vertex {
            position: [half, -half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 1.0], // Bottom-Right
        },
        Vertex {
            position: [half, half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [1.0, 0.0], // Top-Right
        },
        Vertex {
            position: [-half, half, 0.0],
            normal: [0.0, 0.0, 1.0],
            tex_coords: [0.0, 0.0], // Top-Left
        },

        // Back face (facing -Z, CCW winding from behind, normal [0.0, 0.0, -1.0])
        Vertex {
            position: [half, -half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [1.0, 1.0], // Bottom-Left from behind
        },
        Vertex {
            position: [-half, -half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [0.0, 1.0], // Bottom-Right from behind
        },
        Vertex {
            position: [-half, half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [0.0, 0.0], // Top-Right from behind
        },
        Vertex {
            position: [half, half, 0.0],
            normal: [0.0, 0.0, -1.0],
            tex_coords: [1.0, 0.0], // Top-Left from behind
        },
    ];

    let indices = vec![
        // Front face triangles
        0, 1, 2,
        0, 2, 3,

        // Back face triangles
        4, 5, 6,
        4, 6, 7,
    ];

    (vertices, indices)
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
}

impl Primitive {
    pub fn new(device: &wgpu::Device, vertices: Vec<Vertex>, indices: Vec<u32>, label: &str) -> Self {
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
        Self {
            vertices,
            indices,
            vertex_buffer,
            index_buffer,
            num_indices,
        }
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
}

impl Document {
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

        Self {
            scenes: vec![scene],
            nodes: vec![node],
            active_scene_idx: 0,
        }
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

    fn collect_nodes_recursive<'a>(&'a self, node_idx: usize, parent_world: glam::Mat4, list: &mut Vec<(&'a Node, glam::Mat4)>) {
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
                .map_err(|e| format!("Failed to import glTF: {}", e))?
        }
    };

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
                primitives.push(Primitive::new(device, vertices, indices, &prim_label));
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

    Ok(Document {
        scenes,
        nodes,
        active_scene_idx,
    })
}

