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

            let u = s as f32 * s_step;
            // V goes from 0 at south pole to 1 at north pole
            let v = r as f32 * r_step;

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
            indices.push(i1);
            indices.push(i2);

            // Triangle 2
            indices.push(i0);
            indices.push(i2);
            indices.push(i3);
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

    let uvs = [
        [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for face in 0..6 {
        for v in 0..4 {
            vertices.push(Vertex {
                position: positions[face * 4 + v],
                normal: normals[face],
                tex_coords: uvs[v],
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
