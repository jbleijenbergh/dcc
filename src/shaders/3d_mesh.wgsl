struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_position: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient_strength: f32,
    view_transform: f32,
    exposure: f32,
    padding: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct NodeUniform {
    model_matrix: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

@group(2) @binding(0)
var<uniform> node: NodeUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = node.model_matrix * vec4<f32>(model.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;
    out.world_normal = (node.normal_matrix * vec4<f32>(model.normal, 0.0)).xyz;
    out.tex_coords = model.tex_coords;
    return out;
}

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

fn agxDefaultContrastApprox(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2
         - 40.14 * x4 * x
         + 31.96 * x4
         - 6.868 * x2 * x
         + 0.4298 * x2
         + 0.1191 * x
         - vec3<f32>(0.00232);
}

fn agx(val: vec3<f32>) -> vec3<f32> {
    let agx_mat = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104)
    );
    
    let min_ev = -12.47393;
    let max_ev = 4.026069;
    
    // Input transform (inset)
    var col = agx_mat * val;
    
    // Log2 space encoding
    col = clamp(log2(max(col, vec3<f32>(1e-5))), vec3<f32>(min_ev), vec3<f32>(max_ev));
    col = (col - vec3<f32>(min_ev)) / (max_ev - min_ev);
    
    // Apply sigmoid function approximation
    col = agxDefaultContrastApprox(col);
    
    return col;
}

fn agxEotf(val: vec3<f32>) -> vec3<f32> {
    let agx_mat_inv = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116)
    );
    
    // Inverse input transform (outset)
    var col = agx_mat_inv * val;
    
    // sRGB IEC 61966-2-1 2.2 Exponent Reference EOTF Display
    col = pow(max(col, vec3<f32>(0.0)), vec3<f32>(2.2));
    
    return col;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + vec3<f32>(b))) / (x * (c * x + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let object_color: vec4<f32> = textureSample(t_diffuse, s_diffuse, in.tex_coords);

    // Direct lighting computation (PBR-lite / Phong shading)
    let ambient_color = camera.light_color.rgb * camera.ambient_strength;

    let N = normalize(in.world_normal);
    let L = normalize(-camera.light_dir.xyz);
    let diffuse_strength = max(dot(N, L), 0.0);
    let diffuse_color = camera.light_color.rgb * diffuse_strength;

    // Specular lighting (Blinn-Phong style)
    let V = normalize(camera.view_position.xyz - in.world_position);
    let H = normalize(L + V);
    let specular_strength = pow(max(dot(N, H), 0.0), 32.0);
    let specular_color = camera.light_color.rgb * specular_strength * 0.3;

    // Grid outline to visualize UV mappings (subtle styling)
    let u_grid = fract(in.tex_coords.x * 10.0);
    let v_grid = fract(in.tex_coords.y * 10.0);
    let grid_thickness = 0.015;
    var grid_mask: f32 = 1.0;
    if (u_grid < grid_thickness || u_grid > (1.0 - grid_thickness) || v_grid < grid_thickness || v_grid > (1.0 - grid_thickness)) {
        grid_mask = 0.85; // slightly darken grid lines
    }

    let intensity = camera.light_color.a;
    let final_light = (ambient_color + diffuse_color + specular_color) * grid_mask * intensity;
    var shaded_color = final_light * object_color.rgb;

    // Apply exposure
    shaded_color = shaded_color * camera.exposure;

    // Apply View Transform / Tonemapping
    if (camera.view_transform > 1.5) {
        // ACES View Transform
        shaded_color = aces(shaded_color);
        shaded_color = pow(max(shaded_color, vec3<f32>(0.0)), vec3<f32>(2.2));
    } else if (camera.view_transform > 0.5) {
        // AgX View Transform
        shaded_color = agx(shaded_color);
        shaded_color = agxEotf(shaded_color);
    } else {
        // Standard Linear View Transform (no-op, automatically converted to sRGB by swapchain)
    }

    return vec4<f32>(shaded_color, object_color.a);
}
