#version 300 es
// Zehll anti-flashbang: dims the whole screen when its average brightness is high, so a bright
// window does not blind at night. It runs for every pixel of every frame, so it samples sparsely.
precision highp float;

in vec2 v_texcoord;
uniform sampler2D tex;
out vec4 fragColor;

// Average luminance where dimming starts, and how much a fully white screen is dimmed.
const float THRESHOLD = 0.45;
const float MAX_DIM = 0.55;
const int GRID = 4;

void main() {
    vec4 color = texture(tex, v_texcoord);
    float total = 0.0;
    for (int y = 0; y < GRID; y++) {
        for (int x = 0; x < GRID; x++) {
            vec2 uv = (vec2(float(x), float(y)) + 0.5) / float(GRID);
            total += dot(texture(tex, uv).rgb, vec3(0.2126, 0.7152, 0.0722));
        }
    }
    float average = total / float(GRID * GRID);
    float dim = clamp((average - THRESHOLD) / (1.0 - THRESHOLD), 0.0, 1.0) * MAX_DIM;
    fragColor = vec4(color.rgb * (1.0 - dim), color.a);
}
