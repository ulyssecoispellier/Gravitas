use vulkanalia::prelude::v1_3::*;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 2],
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
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        // Field 2: the colour → shader location 1, 3 floats, starts right after pos
        let colour = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(std::mem::size_of::<[f32; 2]>() as u32)
            .build();

        [pos, colour]
    }
}

pub const SQUARE: [Vertex; 4] = [
    Vertex {
        pos: [-0.05, -0.05],
        colour: [0.0, 1.0, 0.0],
    },
    Vertex {
        pos: [0.05, -0.05],
        colour: [0.0, 0.0, 1.0],
    },
    Vertex {
        pos: [0.05, 0.05],
        colour: [1.0, 0.0, 0.0],
    },
    Vertex {
        pos: [-0.05, 0.05],
        colour: [0.0, 1.0, 0.0],
    },
];

pub const SQUARE_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];
