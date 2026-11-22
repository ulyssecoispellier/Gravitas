#version 450

// Full-screen triangle: no vertex buffer, the 3 corners come from gl_VertexIndex
layout(location = 0) out vec2 screen_pos; // -1..1 on screen

void main() {
    // Vertex 0 → (-1,-1), 1 → (3,-1), 2 → (-1,3)
    vec2 corners[3] = vec2[](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    screen_pos = corners[gl_VertexIndex];
    gl_Position = vec4(screen_pos, 0.9999, 1.0);
}