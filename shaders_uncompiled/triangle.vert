#version 450

// The 3 corners, in screen coordinates (-1..1, y pointing down)
vec2 positions[3] = vec2[](
    vec2( 0.0, -0.5),   // top
    vec2( 0.5,  0.5),   // bottom right
    vec2(-0.5,  0.5)    // bottom left
);

// One colour per corner (red, green, blue)
vec3 colors[3] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0)
);

layout(location = 0) out vec3 frag_color;   // sent to the fragment shader

void main() {
    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
    frag_color = colors[gl_VertexIndex];
}