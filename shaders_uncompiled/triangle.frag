#version 450

// Fragment shader: runs once per pixel of each triangle, and decides its colour.

// Inputs from the vertex shader, blended between the triangle's 3 vertices
layout(location = 0) in vec3 frag_color;
layout(location = 1) in vec3 frag_normal;

// Output: the pixel's colour (r, g, b, alpha) in the colour attachment
layout(location = 0) out vec4 out_color;

void main() {
    // Direction TOWARDS the light (from the upper right)
    vec3 light_dir = normalize(vec3(1.0, -1.0, 1.0));
    // Blending between 3 vertices shortens the normal a bit: make it length 1 again
    vec3 normal = normalize(frag_normal);

    // Diffuse light: 1 when facing the light, 0 at 90° or facing away
    float brightness = max(dot(normal, light_dir), 0.0);

    out_color = vec4(frag_color * brightness, 1.0);
}
