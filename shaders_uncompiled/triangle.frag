#version 450

// Fragment shader: runs once per pixel of each triangle, and decides its colour.

// Input from the vertex shader, blended between the triangle's 3 vertices
layout(location = 0) in vec3 frag_color;
// Output: the pixel's colour (r, g, b, alpha) in the colour attachment
layout(location = 0) out vec4 out_color;

void main() {
    out_color = vec4(frag_color, 1.0);
}