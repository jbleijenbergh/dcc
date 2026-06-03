struct FillUniforms {
    base_color: vec4<f32>,
    noise_color: vec4<f32>,
    noise_scale: f32,
    projection_mode: u32, // 0: UV, 1: Triplanar
    udim_tile: u32,
    padding: u32,
};

@group(0) @binding(0) var<uniform> uniforms: FillUniforms;

struct NodeUniform {
    model_matrix: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

@group(1) @binding(0) var<uniform> node: NodeUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Map tile t's U coordinates [t, t+1] to local [0, 1]
    let u_local = model.tex_coords.x - f32(uniforms.udim_tile);
    let v_local = model.tex_coords.y;
    
    // Convert local [0, 1] to clip space [-1, 1]. Flipped Y.
    out.clip_position = vec4<f32>(u_local * 2.0 - 1.0, 1.0 - v_local * 2.0, 0.0, 1.0);
    
    let world_pos = node.model_matrix * vec4<f32>(model.position, 1.0);
    out.world_pos = world_pos.xyz;
    out.world_normal = (node.normal_matrix * vec4<f32>(model.normal, 0.0)).xyz;
    out.tex_coords = model.tex_coords;
    return out;
}

// 3D hash
fn hash3(p: vec3<f32>) -> f32 {
    var pr = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    pr += dot(pr, pr.yzx + 33.33);
    return fract((pr.x + pr.y) * pr.z);
}

// 3D value noise
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    return mix(
        mix(
            mix(hash3(i + vec3<f32>(0.0, 0.0, 0.0)), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x),
            u.y
        ),
        mix(
            mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x),
            u.y
        ),
        u.z
    );
}

// Fractal Brownian Motion (FBM)
fn fbm3(p: vec3<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var temp_p = p;
    for (var i = 0; i < 4; i = i + 1) {
        value += amplitude * noise3(temp_p);
        temp_p = temp_p * 2.0 + vec3<f32>(100.0, 100.0, 100.0);
        amplitude *= 0.5;
    }
    return value;
}

fn triplanar_noise(p: vec3<f32>, n: vec3<f32>, scale: f32) -> f32 {
    let blend = abs(normalize(n));
    let blend_sum = blend.x + blend.y + blend.z;
    let weights = blend / max(blend_sum, 1e-6);

    let sample_x = fbm3(vec3<f32>(p.y * scale, p.z * scale, 0.0));
    let sample_y = fbm3(vec3<f32>(p.x * scale, p.z * scale, 0.5));
    let sample_z = fbm3(vec3<f32>(p.x * scale, p.y * scale, 1.0));

    return sample_x * weights.x + sample_y * weights.y + sample_z * weights.z;
}

fn uv_noise(uv: vec2<f32>, scale: f32) -> f32 {
    return fbm3(vec3<f32>(uv * scale, 0.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var noise_val = 0.0;
    if (uniforms.projection_mode == 1u) {
        // Triplanar projection using world position and normal
        noise_val = triplanar_noise(in.world_pos, in.world_normal, uniforms.noise_scale);
    } else {
        // UV projection using local UV coordinates
        noise_val = uv_noise(in.tex_coords, uniforms.noise_scale);
    }
    
    let final_color = mix(uniforms.base_color, uniforms.noise_color, noise_val);
    return final_color;
}
