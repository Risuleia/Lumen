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
var blurTex : texture_2d<f32>;

@group(0) @binding(1)
var samp : sampler;


// ================================
// FULL WINDOW PILL SDF
// ================================
fn sdf_pill(p: vec2<f32>) -> f32 {
    // aspect stretch so pill = real window shape
    let aspect = 2.5;     // tweak if needed
    var q = p;
    q.x *= aspect;

    // rectangle bounds (-1..1)
    let rect = vec2<f32>(aspect, 1.0);

    // roundness
    let radius = 0.35;

    let d = abs(q) - (rect - vec2<f32>(radius));
    return length(max(d, vec2<f32>(0.0))) - radius;
}

// Normal from SDF
fn sdf_normal(p: vec2<f32>) -> vec2<f32> {
    let e = 0.0025;
    let d  = sdf_pill(p);
    let dx = sdf_pill(p + vec2<f32>(e, 0.0)) - d;
    let dy = sdf_pill(p + vec2<f32>(0.0, e)) - d;
    return normalize(vec2<f32>(dx, dy));
}


@fragment
fn fs_main(input : VSOut) -> @location(0) vec4<f32> {
    // normalized [-1..1]
    let p = input.pos.xy / input.pos.w;

    // Refraction strength
    let strength = 0.02;

    // Edge-accurate normal
    let normal = sdf_normal(p);

    // shift UV like real glass
    let shifted_uv = input.uv + normal * strength;

    let color = textureSample(blurTex, samp, shifted_uv);
    return color;
}