struct StampInstance {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
    @location(3) hardness: f32,
};

struct BrushUniforms {
    resolution: vec2<f32>,
    padding: vec2<f32>,
};
@group(0) @binding(0) var<uniform> uniforms: BrushUniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) center_uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radius: f32,
    @location(4) hardness: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
    instance: StampInstance,
) -> VertexOutput {
    var pos = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0)
    );
    let p = pos[in_vertex_index];
    
    // UV space quad size
    let uv_radius = vec2<f32>(instance.radius / uniforms.resolution.x, instance.radius / uniforms.resolution.y);
    let uv_pos = instance.position + p * uv_radius;
    
    // Convert UV [0, 1] to Clip Space [-1, 1]. Y is flipped in clip space.
    let clip_x = uv_pos.x * 2.0 - 1.0;
    let clip_y = 1.0 - uv_pos.y * 2.0;
    
    var out: VertexOutput;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.uv = uv_pos;
    out.center_uv = instance.position;
    out.color = instance.color;
    out.radius = instance.radius;
    out.hardness = instance.hardness;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance in pixels
    let px_dist = distance(in.uv * uniforms.resolution, in.center_uv * uniforms.resolution);
    if (px_dist > in.radius) {
        discard;
    }
    
    let hardness_radius = in.radius * in.hardness;
    var intensity = 1.0;
    if (px_dist > hardness_radius) {
        let falloff_range = in.radius - hardness_radius;
        if (falloff_range > 0.0) {
            intensity = max(0.0, 1.0 - (px_dist - hardness_radius) / falloff_range);
        } else {
            intensity = 0.0;
        }
    }
    
    let out_a = in.color.a * intensity;
    return vec4<f32>(in.color.rgb * out_a, out_a);
}
