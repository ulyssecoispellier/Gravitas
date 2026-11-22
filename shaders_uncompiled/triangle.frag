#version 450

// Fragment shader: runs once per pixel of each triangle, and decides its colour.

// Inputs from the vertex shader, blended between the triangle's 3 vertices
layout(location = 0) in vec3 frag_color;
layout(location = 1) in vec3 frag_normal;
layout(location = 2) in vec3 frag_world_pos;
layout(location = 3) in vec2 frag_uv;

// Output: the pixel's colour (r, g, b, alpha) in the colour attachment
layout(location = 0) out vec4 out_color;

// Same push constants as the vertex shader (see `PushConstants` in renderer.rs)
layout(push_constant) uniform Push {
    mat4 transform;
    vec4 body_pos;
    vec4 light_pos;
    vec4 info;
} push;

// Rocky planet texture (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D planet_texture;

void main() {
    // Direction from this pixel TOWARDS the star: star position - pixel position, made length 1
    vec3 light_dir = normalize(push.light_pos.xyz - frag_world_pos);
    // Blending between 3 vertices shortens the normal a bit: make it length 1 again
    vec3 normal = normalize(frag_normal);

    // Diffuse light: 1 when facing the light, 0 at 90° or facing away
    float brightness = max(dot(normal, light_dir), 0.0);
    // A glowing body (the star, w = 1) is fully bright everywhere
    brightness = max(brightness, push.info.y);

    // Base colour: texture if info.z = 1, else vertex colour
    vec3 base = mix(frag_color, texture(planet_texture, frag_uv).rgb, push.info.z);

    out_color = vec4(base * brightness, 1.0);
}
