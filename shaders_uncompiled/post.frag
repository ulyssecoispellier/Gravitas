#version 450

// Post-processing pass: reads the finished scene and writes the final image to the screen

// Output: the final pixel colour
layout(location = 0) out vec4 out_color;

// The scene image (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D scene;

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

// ACES tone mapping: squeezes any brightness (0..∞) into 0..1, like a film camera
vec3 tone_map(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    // Off: plain copy of the scene, pixel for pixel
    if (push.settings.x < 0.5) {
        vec3 colour = texelFetch(scene, ivec2(gl_FragCoord.xy), 0).rgb;
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
