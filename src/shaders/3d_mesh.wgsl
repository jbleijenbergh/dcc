struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_position: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.world_position = model.position;
    out.world_normal = model.normal;
    out.tex_coords = model.tex_coords;
    return out;
}

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let object_color: vec4<f32> = textureSample(t_diffuse, s_diffuse, in.tex_coords);

    // Direct lighting computation (PBR-lite / Phong shading)
    let ambient_strength = 0.25;
    let ambient_color = camera.light_color.rgb * ambient_strength;

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

    let final_light = (ambient_color + diffuse_color + specular_color) * grid_mask;
    let shaded_color = final_light * object_color.rgb;

    return vec4<f32>(shaded_color, object_color.a);
}
