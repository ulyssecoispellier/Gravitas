#version 450

layout(location = 0) in vec2 in_pos;
layout(location = 1) in vec3 in_colour;

layout(push_constant) uniform Push {
    vec2 offset;
} push;

layout(location = 0) out vec3 frag_color;

void main() {
    gl_Position = vec4(in_pos + push.offset, 0.0, 1.0);
    frag_color = in_colour;
}