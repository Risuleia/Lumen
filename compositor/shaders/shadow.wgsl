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

// ---- SDF Rounded Rect ----
fn sdf_rounded_rect(p: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - (size - vec2<f32>(radius));
    return length(max(q, vec2<f32>(0.0))) - radius;
}

fn falloff(x: f32) -> f32 {
    return exp(-x * 6.0);
}

@fragment
fn fs_main(input : VSOut) -> @location(0) vec4<f32> {
    var p = input.pos.xy / input.pos.w;

    // slight downward shift = lifted object shadow
    p.y -= 0.035;

    let d = sdf_rounded_rect(p, vec2<f32>(0.45,0.18), 0.09);

    if (d > 0.35) {
        discard;
    }

    let intensity = falloff(abs(d)) * 0.55;

    return vec4<f32>(0.0, 0.0, 0.0, intensity);
}
