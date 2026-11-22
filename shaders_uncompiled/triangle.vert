#version 450

// Vertex shader: runs once per vertex. Places it on screen and passes its colour on.

// Inputs: one vertex from the vertex buffer (see `Vertex` in mesh.rs)
layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_colour;

// The body's transform (projection x view x model), sent with a push constant
layout(push_constant) uniform Push { mat4 transform; } push;

// Output to the fragment shader (blended across the triangle)
layout(location = 0) out vec3 frag_color;

void main() {
    // Position on screen (clip space). 1.0 = it's a point, so translations apply
    gl_Position = push.transform * vec4(in_pos, 1.0);
    frag_color = in_colour;
}