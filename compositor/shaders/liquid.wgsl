struct VSOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv : vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) in_pos : vec2<f32>,
    @location(1) in_uv  : vec2<f32>
) -> VSOut {
    var out : VSOut;
    out.pos = vec4<f32>(in_pos, 0.0, 1.0);
    out.uv = in_uv;
    return out;
}

@group(0) @binding(0)
var tex : texture_2d<f32>;

@group(0) @binding(1)
var samp : sampler;

struct IslandParams {
    radius : f32,
    refraction : f32,
    glow_power : f32,
    shadow_power : f32,
};

@group(0) @binding(2)
var<uniform> params : IslandParams;

struct RegionParams {
    window_pos : vec2<f32>,
    window_size : vec2<f32>,
    capture_size : vec2<f32>,
    _pad : vec2<f32>,
};

@group(0) @binding(3)
var<uniform> region : RegionParams;


// -------------------- SDF --------------------
fn roundrect_sdf(uv: vec2<f32>) -> f32 {
    let p = uv * region.window_size - region.window_size * 0.5;

    let hx = region.window_size.x * 0.5;
    let hy = region.window_size.y * 0.5;

    let r = 26.0;

    let q = abs(p) - vec2<f32>(hx - r, hy - r);
    return length(max(q, vec2<f32>(0.0))) - r;
}

fn sdf_world(uv: vec2<f32>) -> f32 {
    return roundrect_sdf(uv);
}


// -------------------- Normal --------------------
fn sdf_normal(uv: vec2<f32>) -> vec2<f32> {
    let e = 0.003;
    let d = sdf_world(uv);

    let dx = sdf_world(uv + vec2<f32>(e, 0.0)) - d;
    let dy = sdf_world(uv + vec2<f32>(0.0, e)) - d;

    let g = vec2<f32>(dx, dy);
    let len = max(length(g), 0.0001);
    return g / len;
}


// -------------------- Fragment --------------------
fn gaussianBlur(coord: vec2<f32>, radius: f32) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    var total = 0.0;

    // 5x5 Kernel like article (hardcoded loop count → WGSL friendly)
    for (var x: i32 = -2; x <= 2; x = x + 1) {
        for (var y: i32 = -2; y <= 2; y = y + 1) {
            let dx = f32(x);
            let dy = f32(y);

            let offset = vec2<f32>(dx, dy) * radius;

            // Gaussian weight
            let weight = exp(-0.5 * (dx*dx + dy*dy) / 2.0);

            let uv = clamp(
                (coord + offset) / region.capture_size,
                vec2<f32>(0.0),
                vec2<f32>(1.0)
            );

            color += textureSample(tex, samp, uv).rgb * weight;
            total += weight;
        }
    }

    return color / total;
}


@fragment
fn fs_main(input: VSOut) -> @location(0) vec4<f32> {
    let frag = vec2<f32>(
        region.window_pos.x + input.uv.x * region.window_size.x,
        region.window_pos.y + input.uv.y * region.window_size.y
    );

    let glass_size = region.window_size;
    let glass_center = region.window_pos + glass_size * 0.5;
    let glass_coord = frag - glass_center;

    let size = min(glass_size.x, glass_size.y);
    let dist = sdf_world(input.uv);
    let inversedSDF = -dist / size;

    // outside → passthrough
    if (inversedSDF < 0.0) {
        let uv = clamp(
            frag / region.capture_size,
            vec2<f32>(0.0), vec2<f32>(1.0)
        );
        let base = textureSample(tex, samp, uv).rgb;
        return vec4<f32>(base, 1.0);
    }

    let dir = normalize(glass_coord);

    // distortion curve
    let distFromCenter = 1.0 - clamp(inversedSDF / 0.3, 0.0, 1.0);
    let distortion = 1.0 - sqrt(max(1.0 - distFromCenter * distFromCenter, 0.0));
    let offset = distortion * dir * (glass_size * 0.5);
    let coord = frag - offset;


    // ----- Gaussian blur radius -----
    let blurIntensity = 1.2;
    let blurRadius = blurIntensity * (1.0 - distFromCenter * 0.5);

    // ----- chromatic aberration -----
    let edge = smoothstep(0.0, 0.02, inversedSDF);
    let shift = dir * edge * 3.0;

    let r = gaussianBlur(coord - shift, blurRadius).r;
    let g = gaussianBlur(coord, blurRadius).g;
    let b = gaussianBlur(coord + shift, blurRadius).b;

    var glass = vec3<f32>(r,g,b);

    // -------- soft inner rim highlight -------- 
    // thin bright ribbon hugging edge - now wider 
    let rim = smoothstep(-8.0, -0.6, dist);

    // Direction basis 
    let uv_center = input.uv - vec2<f32>(0.5, 0.5);
    let rim_dir = normalize(uv_center);
    // Primary bright lobe (top-left) - boosted intensity 
    let light_dir = normalize(vec2<f32>(-0.75, -0.65));
    let light_strength = clamp(dot(rim_dir, light_dir), 0.0, 1.0); 
    let primary = rim * pow(light_strength, 1.8) * 0.6 * params.glow_power;
    // Secondary softer lobe (bottom-right) - boosted intensity 
    let secondary_dir = normalize(vec2<f32>(0.9, 0.8)); 
    let secondary_strength = clamp(dot(rim_dir, secondary_dir), 0.0, 1.0); 
    let secondary = rim * pow(secondary_strength, 1.8) * 0.35 * params.glow_power; 
    glass += primary + secondary;

    // ------- adaptive tinting --------
    let base_uv = clamp(
        frag / region.capture_size,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );

    let bg = textureSample(tex, samp, base_uv).rgb;

    // luminance perception weights
    let luminance = dot(bg, vec3<f32>(0.299, 0.587, 0.114));
    let response = smoothstep(0.25, 0.9, luminance);

    // dark tint for bright bg, light tint for dark bg
    let darkTint = vec3<f32>(0.42, 0.48, 0.58);   // smoked glass
    let lightTint = vec3<f32>(1.08, 1.08, 1.10);   // gentle lift

    let adaptiveTint = mix(lightTint, darkTint, response);

    // apply
    glass *= adaptiveTint;
    
    glass *= vec3<f32>(0.90); // tint

    return vec4<f32>(glass, 1.0);
}
