#version 450

// egui pass: colours each pixel of egui's triangles = vertex colour × font atlas
// (letters: their shape in the atlas; plain boxes: a white atlas pixel, so just the colour)

// From egui.vert, interpolated across the triangle
layout(location = 0) in vec2 in_uv;
layout(location = 1) in vec4 in_colour;

// egui's font atlas (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D font;

// Final colour, blended over the image (premultiplied alpha)
layout(location = 0) out vec4 out_colour;

void main() {
    out_colour = in_colour * texture(font, in_uv);
}
