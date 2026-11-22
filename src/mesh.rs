use std::f32::consts::PI;
use vulkanalia::prelude::v1_3::*;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub colour: [f32; 3],
}

impl Vertex {
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 2] {
        // Field 1: the position → shader location 0, 2 floats, starts at byte 0
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

        [pos, colour]
    }
}

pub const SQUARE: [Vertex; 4] = [
    Vertex {
        pos: [-0.05, -0.05, 0.0],
        colour: [0.0, 1.0, 0.0],
    },
    Vertex {
        pos: [0.05, -0.05, 0.0],
        colour: [0.0, 0.0, 1.0],
    },
    Vertex {
        pos: [0.05, 0.05, 0.0],
        colour: [1.0, 0.0, 0.0],
    },
    Vertex {
        pos: [-0.05, 0.05, 0.0],
        colour: [0.0, 1.0, 0.0],
    },
];
pub const SQUARE_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

pub fn uv_sphere(
    segments: u32,
    rings: u32,
    radius: f32,
    _colour: [f32; 3],
) -> (Vec<Vertex>, Vec<u16>) {
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
            });
        }
    }

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
