// ===============================
// Liquid Glass Compositor Shader - FIXED
// ===============================

struct VSOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv : vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) in_pos : vec2<f32>,
    @location(1) in_uv  : vec2<f32>
) -> VSOut {
    var out : VSOut;
    out.pos = vec4<f32>(in_pos, 0.0, 1.0);
    out.uv  = in_uv;
    return out;
}

// ---------- BINDINGS ----------

@group(0) @binding(0)
var refracted_tex : texture_2d<f32>;

@group(0) @binding(1)
var glow_tex : texture_2d<f32>;

@group(0) @binding(2)
var shadow_tex : texture_2d<f32>;

@group(0) @binding(3)
var samp : sampler;

struct IslandParams {
    scale: f32,
    radius: f32,
    glow_power: f32,
    shadow_power: f32,
};

@group(0) @binding(4)
var<uniform> params: IslandParams;

struct RegionParams {
    window_pos : vec2<f32>,
    window_size : vec2<f32>,
    capture_size : vec2<f32>,
    _pad : vec2<f32>,
};

@group(0) @binding(5)
var<uniform> region : RegionParams;

// ---------- Rounded Mask ----------
fn rounded_mask(uv : vec2<f32>) -> f32 {
    var p = uv * 2.0 - 1.0;

    // Fix aspect distortion (otherwise pill becomes ellipse)
    let aspect = region.window_size.x / region.window_size.y;
    p.x /= aspect;

    // Rectangle half-extents in normalized space
    let rect = vec2<f32>(1.0, 1.0);

    // Convert pixel radius → normalized radius
    let radius = params.radius / region.window_size.y;

    let q = abs(p) - rect + vec2<f32>(radius);
    let dist = length(max(q, vec2<f32>(0.0))) - radius;

    // Soft edge = liquid smooth Apple-like edge
    return smoothstep(0.02, -0.02, dist);
}

@fragment
fn fs_main(input: VSOut) -> @location(0) vec4<f32> {
    let mask = rounded_mask(input.uv);
    // let mask = 1.0;

    let pixel_x = region.window_pos.x + input.uv.x * region.window_size.x;
    let pixel_y = region.window_pos.y + input.uv.y * region.window_size.y;

    var screen_uv = vec2<f32>(
        pixel_x / region.capture_size.x,
        pixel_y / region.capture_size.y
    );

    let uv = clamp(screen_uv, vec2<f32>(0.0), vec2<f32>(1.0));

    let refr = textureSample(refracted_tex, samp, uv);
    let glow = textureSample(glow_tex, samp, input.uv) * params.glow_power;
    let shadow = textureSample(shadow_tex, samp, input.uv) * params.shadow_power;

    let shadowed_rgb = mix(refr.rgb, shadow.rgb, shadow.a);
    var final_rgb = shadowed_rgb + glow.rgb * glow.a;

    return vec4<f32>(final_rgb * mask, mask);
}