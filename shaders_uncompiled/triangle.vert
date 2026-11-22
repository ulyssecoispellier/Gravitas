#version 450

// Vertex shader: runs once per vertex. Places it on screen and passes its colour and normal on.

// Inputs: one vertex from the vertex buffer (see `Vertex` in mesh.rs)
layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_colour;
layout(location = 2) in vec3 in_normal;
layout(location = 3) in vec2 in_uv;

// The body's transform (projection x view x model), sent with a push constant
layout(push_constant) uniform Push {
    mat4 transform;
    vec4 body_pos;
    vec4 light_pos;
    vec4 info;
} push;

// Outputs to the fragment shader (blended across the triangle)
layout(location = 0) out vec3 frag_color;
layout(location = 1) out vec3 frag_normal;
layout(location = 2) out vec3 frag_world_pos;

void main() {
    // Position on screen (clip space). 1.0 = it's a point, so translations apply
    gl_Position = push.transform * vec4(in_pos, 1.0);
    frag_color = in_colour;
    // The model matrix only moves the sphere (no rotation), so the normal stays valid as is
    frag_normal = in_normal;
    
    frag_world_pos = in_pos * push.info.x + push.body_pos.xyz;
}
