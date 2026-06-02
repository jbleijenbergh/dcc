struct LayerInfo {
    opacity: f32,
    blend_mode: u32, // 0: Normal, 1: Multiply, 2: Add
    visible: u32,
    padding: u32,
};

struct LayersUniform {
    count: u32,
    padding1: u32,
    padding2: u32,
    padding3: u32,
    layers: array<LayerInfo, 16>,
};

@group(0) @binding(0) var<uniform> uniforms: LayersUniform;
@group(0) @binding(1) var layer_tex: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    // Full screen triangle
    var out: VertexOutput;
    let x = f32((in_vertex_index << 1u) & 2u);
    let y = f32(in_vertex_index & 2u);
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Background color
    var b_r = 230.0 / 255.0;
    var b_g = 230.0 / 255.0;
    var b_b = 230.0 / 255.0;
    var b_a = 1.0;
    
    for (var i = 0u; i < uniforms.count; i = i + 1u) {
        let layer = uniforms.layers[i];
        if (layer.visible == 0u || layer.opacity <= 0.0) {
            continue;
        }
        
        let tex_color = textureSample(layer_tex, samp, in.uv, i);
        let l_a = tex_color.a * layer.opacity;
        if (l_a <= 0.0001) {
            continue;
        }
        
        // Un-premultiply the layer color (we store it premultiplied)
        var l_r = 0.0; var l_g = 0.0; var l_b = 0.0;
        if (tex_color.a > 0.0) {
            l_r = tex_color.r / tex_color.a;
            l_g = tex_color.g / tex_color.a;
            l_b = tex_color.b / tex_color.a;
        }
        
        // Blend mode
        var blend_r = l_r;
        var blend_g = l_g;
        var blend_b = l_b;
        
        if (layer.blend_mode == 1u) { // Multiply
            blend_r = l_r * b_r;
            blend_g = l_g * b_g;
            blend_b = l_b * b_b;
        } else if (layer.blend_mode == 2u) { // Add
            blend_r = min(1.0, l_r + b_r);
            blend_g = min(1.0, l_g + b_g);
            blend_b = min(1.0, l_b + b_b);
        }
        
        // Composite back using standard over (since b is currently un-premultiplied)
        let out_a = l_a + b_a * (1.0 - l_a);
        if (out_a > 0.0) {
            b_r = (blend_r * l_a + b_r * b_a * (1.0 - l_a)) / out_a;
            b_g = (blend_g * l_a + b_g * b_a * (1.0 - l_a)) / out_a;
            b_b = (blend_b * l_a + b_b * b_a * (1.0 - l_a)) / out_a;
        }
        b_a = out_a;
    }
    
    // Output standard rgba
    return vec4<f32>(b_r, b_g, b_b, b_a);
}
