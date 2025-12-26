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
    out.uv = uvs[vid];
    return out;
}

@group(0) @binding(0)
var tex : texture_2d<f32>;

@group(0) @binding(1)
var samp : sampler;

@fragment
fn downsample(input: VSOut) -> @location(0) vec4<f32> {
    let uv = input.uv;
    let offset = 1.0 / 256.0;

    var c =
        textureSample(tex, samp, uv + vec2<f32>( offset,  offset)) +
        textureSample(tex, samp, uv + vec2<f32>(-offset,  offset)) +
        textureSample(tex, samp, uv + vec2<f32>( offset, -offset)) +
        textureSample(tex, samp, uv + vec2<f32>(-offset, -offset));

    return c * 0.25;
}

@fragment
fn upsample(input: VSOut) -> @location(0) vec4<f32> {
    let uv = input.uv;
    let offset = 1.0 / 256.0;

    var c =
        textureSample(tex, samp, uv) * 0.5 +
        textureSample(tex, samp, uv + vec2<f32>( offset, 0.0)) * 0.125 +
        textureSample(tex, samp, uv + vec2<f32>(-offset, 0.0)) * 0.125 +
        textureSample(tex, samp, uv + vec2<f32>(0.0, offset)) * 0.125 +
        textureSample(tex, samp, uv + vec2<f32>(0.0,-offset)) * 0.125;

    return c;
}