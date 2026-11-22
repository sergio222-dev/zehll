// Streak left behind by the workspace disc as it slides to another slot: a capsule from the tail
// to the head that widens toward the head, accent at the tail and hotter at the head, with a glow.

struct Uniforms {
    // Widget size in logical pixels.
    size: vec2<f32>,
    // Centers of the tail and of the disc on the x axis; the streak runs along the middle row.
    tail: f32,
    head: f32,
    // 1 while the disc moves, down to 0 as the trail fades out.
    fade: f32,
    // Half the streak's thickness at the tail and at the head.
    thin: f32,
    thick: f32,
    // How far the glow reaches past the streak.
    glow: f32,
    // The theme accent.
    color: vec4<f32>,
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * u.size;
    let y = p.y - u.size.y * 0.5;
    let span = u.head - u.tail;
    if abs(span) < 0.5 {
        return vec4<f32>(0.0);
    }

    // 0 at the tail, 1 at the head, whichever way the disc moves.
    let along = clamp((p.x - u.tail) / span, 0.0, 1.0);
    let dx = p.x - (u.tail + along * span);
    let half = mix(u.thin, u.thick, along * along);
    // Signed distance to the tapered capsule: negative inside it.
    let distance = length(vec2<f32>(dx, y)) - half;

    let body = 1.0 - smoothstep(-0.75, 0.75, distance);
    let glow = (1.0 - smoothstep(0.0, u.glow, distance)) * 0.45;
    // Strongest right behind the disc, gone at the tail.
    let strength = pow(along, 0.8) * u.fade;
    let alpha = clamp(body + glow, 0.0, 1.0) * strength;

    let hot = mix(u.color.rgb, vec3<f32>(1.0, 0.95, 0.85), 0.5);
    let color = mix(u.color.rgb, hot, smoothstep(0.5, 1.0, along) * body);
    // Premultiplied, as the pipeline blends.
    return vec4<f32>(color * alpha, alpha);
}
