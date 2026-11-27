#version 450

// egui pass: one vertex of egui's triangles (the Celestial Bodies panel), from points to screen

// One egui vertex (epaint::Vertex, 20 bytes): position in points, font-atlas uv, colour
layout(location = 0) in vec2 in_pos;    // points, (0, 0) = top-left of the window
layout(location = 1) in vec2 in_uv;     // 0..1 in the font atlas
layout(location = 2) in vec4 in_colour; // sRGB, premultiplied alpha, 0..1

// Window size in points (swapchain extent / pixels_per_point)
layout(push_constant) uniform Push {
    vec2 size;
} push;

// Passed on to egui.frag
layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec4 out_colour;

void main() {
    out_uv = in_uv;
    out_colour = in_colour;

    // Points → clip space: 0..size becomes -1..+1 (y already points down in Vulkan, like egui)
    vec2 clip = 2.0 * in_pos / push.size - 1.0;
    gl_Position = vec4(clip, 0.0, 1.0);
}
