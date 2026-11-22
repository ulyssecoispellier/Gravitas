//! Geometry: the vertex format shared with the shaders, and mesh generators.

use std::f32::consts::PI;
use vulkanalia::prelude::v1_3::*;

/// One point of a mesh, as the GPU receives it.
///
/// `#[repr(C)]` keeps the fields in this exact order in memory (Rust may reorder them otherwise),
/// so the layout matches what we describe to Vulkan below.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    /// Position in model space (x, y, z) → shader `location = 0`
    pub pos: [f32; 3],
    /// Colour (r, g, b), each 0.0–1.0 → shader `location = 1`
    pub colour: [f32; 3],
    /// Direction the surface faces at this point, length 1 → shader `location = 2`
    pub normal: [f32; 3],
    /// Texture coordinates (u, v), each 0.0–1.0: which point of the image this vertex shows → shader `location = 3`
    pub uv: [f32; 2],
}

impl Vertex {
    /// How vertices are laid out in the vertex buffer: one `Vertex` after another.
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    /// Where each field of a `Vertex` is, so the vertex shader can read it.
    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 4] {
        // Field 1: the position → shader location 0, 3 floats, starts at byte 0
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)
            .build();

        // Field 2: the colour → shader location 1, 3 floats, starts right after pos
        let colour = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(std::mem::size_of::<[f32; 3]>() as u32)
            .build();

        // Field 3: the normal → shader location 2, 3 floats, starts right after colour
        let normal = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(2 * std::mem::size_of::<[f32; 3]>() as u32)
            .build();

        // Field 4: the UV → shader location 3, 2 floats, starts right after normal
        let uv = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(3)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(3 * std::mem::size_of::<[f32; 3]>() as u32)
            .build();

        [pos, colour, normal, uv]
    }
}

/// Generates a UV sphere: like a globe, rings of latitude × segments of longitude.
///
/// Returns the vertices and the indices that link them into triangles.
/// `_colour` is not used yet: for now the colour comes from the position.
pub fn uv_sphere(
    segments: u32,
    rings: u32,
    radius: f32,
    _colour: [f32; 3],
) -> (Vec<Vertex>, Vec<u16>) {
    // 1. Vertices: one per (ring, segment) crossing
    let mut vertices = Vec::new();

    for ring in 0..=rings {
        // angle from the top of the sphere: 0 (north pole) → π (south pole)
        let phi = PI * ring as f32 / rings as f32;

        for seg in 0..=segments {
            // angle around the ring: 0 → 2π (a full turn), like the circle example
            let theta = 2.0 * PI * seg as f32 / segments as f32;

            let x = radius * phi.sin() * theta.cos();
            let y = radius * phi.sin() * theta.sin();
            let z = radius * phi.cos();

            // PLACEHOLDER : Shading not yet included, a colour variable will be used later (Current will be a rand colour)
            vertices.push(Vertex {
                pos: [x, y, z],
                colour: [
                    x / radius * 0.5 + 0.5,
                    y / radius * 0.5 + 0.5,
                    z / radius * 0.5 + 0.5,
                ],
                normal: [x / radius, y / radius, z / radius],
                // u goes around the sphere, v from the north pole (0) to the south pole (1)
                uv: [seg as f32 / segments as f32, ring as f32 / rings as f32],
            });
        }
    }

    // 2. Indices: each little square of the grid = 2 triangles
    let mut indices = Vec::new();
    let row = segments + 1; // number of points on each ring

    for ring in 0..rings {
        for seg in 0..segments {
            // the 4 corners of this little square, as vertex numbers
            let a = ring * row + seg;
            let b = a + 1;
            let c = a + row + 1;
            let d = a + row;

            // triangle 1
            indices.push(a as u16);
            indices.push(b as u16);
            indices.push(c as u16);
            // triangle 2: shares the diagonal a–c with triangle 1
            indices.push(a as u16);
            indices.push(c as u16);
            indices.push(d as u16);
        }
    }

    (vertices, indices)
}
