struct VSOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid : u32) -> VSOut {
    var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
    );

    var out : VSOut;
    out.pos = vec4<f32>(positions[vid], 0.0, 1.0);
    out.uv  = uvs[vid];
    return out;
}

@group(0) @binding(0)
var maskTex : texture_2d<f32>;

@group(0) @binding(1)
var samp : sampler;


// ===================================
// FULL WINDOW PILL SDF
// ===================================
fn sdf_pill(p: vec2<f32>) -> f32 {
    // stretch horizontally → pill
    let aspect = 2.5;          // tune if window proportions change
    var q = p;
    q.x *= aspect;

    let rect = vec2<f32>(aspect, 1.0);
    let radius = 0.55;         // roundness

    let d = abs(q) - (rect - vec2<f32>(radius));
    return length(max(d, vec2<f32>(0.0))) - radius;
}

// smooth glow falloff
fn falloff(d: f32) -> f32 {
    return exp(-d * 6.0);
}

@fragment
fn fs_main(input : VSOut) -> @location(0) vec4<f32> {
    let p = input.uv * 2.0 - 1.0;

    let d = sdf_pill(p);

    let edge = 1.0 - smoothstep(-0.06, 0.06, d);

    let inner_kill = smoothstep(-0.25, -0.02, d);

    let glow_mask = edge * inner_kill;

    let glow_color = vec3<f32>(0.85, 0.92, 1.2);

    return vec4<f32>(glow_color * glow_mask, glow_mask * 0.55);

}
