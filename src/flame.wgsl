// Flames licking out of the edges of the media button, burned once when playback starts or stops.

struct Uniforms {
    // Widget size in logical pixels.
    size: vec2<f32>,
    // Half the button's size: the flames start at its edges.
    half_extent: vec2<f32>,
    // 0 to 1 over the transition.
    progress: f32,
    // Seconds since the transition started; scrolls the noise.
    time: f32,
    // 1 when playback started (a hot burst), 0 when it stopped (dying embers).
    playing: f32,
    // How far the flames reach past the edges, in logical pixels.
    reach: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// One triangle covering the viewport, which iced sets to the widget's bounds.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOutput;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash(cell);
    let b = hash(cell + vec2<f32>(1.0, 0.0));
    let c = hash(cell + vec2<f32>(0.0, 1.0));
    let d = hash(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var octave = 0; octave < 3; octave++) {
        value += amplitude * noise(q);
        q *= 2.03;
        amplitude *= 0.5;
    }
    return value;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - 0.5) * u.size;
    // Signed distance to the button's square: negative inside it.
    let q = abs(p) - u.half_extent;
    let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
    if distance < 0.0 {
        return vec4<f32>(0.0);
    }

    let t = clamp(u.progress, 0.0, 1.0);
    // Both directions flare up fast and die down; stopping burns lower and cooler.
    let envelope = smoothstep(0.0, 0.15, t) * (1.0 - smoothstep(0.45, 1.0, t)) * mix(0.8, 1.0, u.playing);

    // Noise sampled on a rotating circle, so the tongues travel around the border without a seam,
    // and pushed outward over time, so they rise away from the edges.
    let angle = atan2(p.y, p.x) + u.time * 1.6;
    let around = vec2<f32>(cos(angle), sin(angle)) * 2.6;
    let n = fbm(around + vec2<f32>(0.0, distance * 0.35 - u.time * 5.0));
    let height = u.reach * envelope * (0.3 + 1.1 * n);
    // Tongues under a pixel and a half fade out: otherwise the pixels right on the edge keep full
    // heat while the reach shrinks to nothing and leave a line behind.
    var heat = clamp(1.0 - distance / max(height, 0.001), 0.0, 1.0) * smoothstep(0.0, 1.5, height);
    // Stopping burns cooler: no white core, only red embers.
    heat *= mix(0.8, 1.0, u.playing);

    let ember = vec3<f32>(0.75, 0.08, 0.02);
    let flame = vec3<f32>(1.0, 0.45, 0.05);
    let core = vec3<f32>(1.0, 0.92, 0.6);
    var color = mix(ember, flame, smoothstep(0.15, 0.55, heat));
    color = mix(color, core, smoothstep(0.6, 1.0, heat));
    let alpha = smoothstep(0.0, 0.3, heat);
    // Premultiplied, as the pipeline blends.
    return vec4<f32>(color * alpha, alpha);
}
