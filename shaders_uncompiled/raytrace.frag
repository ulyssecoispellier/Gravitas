#version 450

// Ray-tracing pass: runs once per pixel of the screen

// Position on screen, -1..1 (from raytrace.vert)
layout(location = 0) in vec2 screen_pos;

layout(location = 0) out vec4 out_color;

void main() {
    // Test colour: red = x, green = y
    out_color = vec4(screen_pos * 0.5 + 0.5, 0.0, 1.0);
}