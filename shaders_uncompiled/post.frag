#version 450

// Post-processing pass: reads the finished scene and writes the final image to the screen

// Output: the final pixel colour
layout(location = 0) out vec4 out_color;

// The scene image (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D scene;
// The bright light already blurred along x by bloom.frag (set 0, binding 2)
layout(set = 0, binding = 2) uniform sampler2D bloom_h;

// Same layout as `PostPushConstants` in renderer.rs
layout(push_constant) uniform Push {
    vec4 settings; // x = retro dithering on (1) / off (0)
} push;

// 4×4 Bayer matrix: thresholds 0..15, ordered so neighbouring pixels get very different values
const float BAYER[16] = float[](
     0.0,  8.0,  2.0, 10.0,
    12.0,  4.0, 14.0,  6.0,
     3.0, 11.0,  1.0,  9.0,
    15.0,  7.0, 13.0,  5.0
);
// Size of one retro pixel, in screen pixels
const float PIXEL_SIZE = 3.0;
// Colour levels per channel (2 = 8 colours in total)
const float LEVELS = 2.0;
// How much of the glow is added on top of the scene
const float BLOOM_STRENGTH = 0.6;
// Samples on each side of the pixel: the loop reads 2 * RADIUS + 1 = 13 pixels
const int BLOOM_RADIUS = 6;
// Screen pixels between two samples (wider glow for the same number of reads)
const float BLOOM_SPACING = 4.0;

// ACES tone mapping: squeezes any brightness (0..∞) into 0..1, like a film camera
vec3 tone_map(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Bloom pass 2: blurs bloom_h along y, which finishes the 2D Gaussian glow
vec3 bloom(vec2 uv) {
    vec3 total_light = vec3(0.0);
    float total_weight = 0.0;
    vec2 texel = 1.0 / vec2(textureSize(scene, 0));

    for (int y = -BLOOM_RADIUS; y <= BLOOM_RADIUS; y++) {
        vec2 offset = vec2(0.0, y) * BLOOM_SPACING * texel;
        vec3 neighbour = texture( bloom_h, clamp(uv + offset, 0.0, 1.0) ).rgb;
        // Gaussian denominator 2*sigma^2, with sigma = BLOOM_RADIUS / 2
        float half_bloom_rad_squared = float(BLOOM_RADIUS) * float(BLOOM_RADIUS) / 2.0;
        float weight = exp(-(float(y) * float(y)) / half_bloom_rad_squared);

        total_light += neighbour * weight;
        total_weight += weight;
    }
    return total_light / total_weight;
}

void main() {
    // Off: plain copy of the scene, pixel for pixel
    if (push.settings.x < 0.5) {
        vec3 colour = texelFetch(scene, ivec2(gl_FragCoord.xy), 0).rgb;
        vec2 uv = gl_FragCoord.xy / vec2(textureSize(scene, 0));
        colour += bloom(uv) * BLOOM_STRENGTH;
        out_color = vec4(tone_map(colour), 1.0);
        return;
    }

    // Big retro pixels: every PIXEL_SIZE × PIXEL_SIZE block reads the same scene pixel
    ivec2 block = ivec2(gl_FragCoord.xy / PIXEL_SIZE);
    vec3 colour = tone_map(texelFetch(scene, ivec2(vec2(block) * PIXEL_SIZE), 0).rgb);

    // Work in sRGB (what the screen shows), so the pattern looks even to the eye
    vec3 srgb = pow(colour, vec3(1.0 / 2.2));

    // This block's threshold in the repeating 4×4 pattern, between 0 and 1
    int index = (block.x % 4) + (block.y % 4) * 4;
    float threshold = (BAYER[index] + 0.5) / 16.0;

    // Quantise each channel to LEVELS steps: round up or down depending on the threshold
    vec3 stepped = floor(srgb * (LEVELS - 1.0) + threshold) / (LEVELS - 1.0);

    // Back to linear (the sRGB swapchain converts again on output)
    out_color = vec4(pow(clamp(stepped, 0.0, 1.0), vec3(2.2)), 1.0);
}
