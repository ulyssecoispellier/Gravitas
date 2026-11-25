#version 450

// Bloom pass 1: keeps only the very bright light and blurs it along x (one row)
// post.frag then blurs this result along y: 33 + 33 reads instead of 33 * 33

// Output: the glow blurred along x, written into bloom_image
layout(location = 0) out vec4 out_color;

// The scene image (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D scene;

// Same layout as `PostPushConstants` in renderer.rs
layout(push_constant) uniform Push {
    vec4 settings; // x = retro dithering on (1) / off (0), y = render scale (1.0 full, 0.75 with H)
} push;

// Only light above this brightness glows (1.0 = brighter than white)
const float BLOOM_THRESHOLD = 1.0;
// Samples on each side of the pixel: the loop reads 2 * RADIUS + 1 = 33 pixels
const int BLOOM_RADIUS = 16;
// Screen pixels between two samples: 1.5 lands between pixels, so the LINEAR sampler blends 2 (no dotted grid)
const float BLOOM_SPACING = 1.5;

// Keeps only the light above BLOOM_THRESHOLD (the part that glows)
vec3 bright(vec3 c) {
    // A NaN or infinite pixel would spread over the whole bloom square: treat it as black
    if (any(isnan(c)) || any(isinf(c))) { return vec3(0.0); }
    // Firefly cap: a rare broken sample can't light up a whole bloom square (no real pixel is this bright)
    c = min(c, vec3(100.0));
    return max(c - BLOOM_THRESHOLD, 0.0);
}

// Gaussian blur of the bright light along x only
vec3 bloom(vec2 uv) {
    vec3 total_light = vec3(0.0);
    float total_weight = 0.0;
    vec2 texel = 1.0 / vec2(textureSize(scene, 0));

    for (int x = -BLOOM_RADIUS; x <= BLOOM_RADIUS; x++) {
        vec2 offset = vec2(x, 0.0) * BLOOM_SPACING * texel;
        vec3 neighbour = bright( texture( scene, clamp(uv + offset, 0.0, push.settings.y) ).rgb );
        // Gaussian denominator 2*sigma^2, with sigma = BLOOM_RADIUS / 2
        float half_bloom_rad_squared = float(BLOOM_RADIUS) * float(BLOOM_RADIUS) / 2.0;
        float weight = exp(-(float(x) * float(x)) / half_bloom_rad_squared);

        total_light += neighbour * weight;
        total_weight += weight;
    }
    return total_light / total_weight;
}

void main() {
    vec2 uv = gl_FragCoord.xy / vec2(textureSize(scene, 0));
    out_color = vec4(bloom(uv), 1.0);
}