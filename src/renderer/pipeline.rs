//! Shaders and the graphics pipeline: every drawing setting, baked once.

use anyhow::Result;
use vulkanalia::prelude::v1_3::*;

use super::{DEPTH_FORMAT, PushConstants};
use crate::mesh::Vertex;

/// Wraps compiled SPIR-V bytes into a shader module Vulkan can use.
pub(super) fn create_shader_module(device: &Device, bytes: &[u8]) -> Result<vk::ShaderModule> {
    // Copies the bytes into an aligned buffer: SPIR-V is read as 4-byte words
    let bytecode = vulkanalia::bytecode::Bytecode::new(bytes)?;
    let info = vk::ShaderModuleCreateInfo::builder()
        .code_size(bytecode.code_size())
        .code(bytecode.code());

    Ok(unsafe { device.create_shader_module(&info, None)? })
}

/// Builds the graphics pipeline: every drawing setting, baked once for speed.
///
/// Returns the layout (what data the shaders receive) and the pipeline itself.
pub(super) fn create_pipeline(
    device: &Device,
    format: vk::Format,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
    set_layout: vk::DescriptorSetLayout,
    with_vertices: bool,
    with_depth: bool,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // The 2 shaders, both starting at their `main` function
    let stages = &[
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(b"main\0"),
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(b"main\0"),
    ];

    // Vertex format: `Vertex` from mesh.rs, or none (the full-screen pass makes its own vertices)
    let bindings = &[Vertex::binding_description()];
    let attributes = Vertex::attribute_descriptions();
    let vertex_input = if with_vertices {
        vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(bindings)
            .vertex_attribute_descriptions(&attributes)
    } else {
        vk::PipelineVertexInputStateCreateInfo::builder()
    };

    // Every 3 indices = 1 triangle
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    // 1 viewport and 1 scissor, set each frame (see the dynamic states below)
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1);

    // Filled triangles, draw both faces
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::CLOCKWISE);

    // Depth test (if used): keep a pixel only if it's closer (LESS) than what's stored, then store its depth
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(with_depth)
        .depth_write_enable(with_depth)
        .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);

    // No anti-aliasing (1 sample per pixel)
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::_1);

    // No blending: new colours replace the old ones
    let blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(vk::ColorComponentFlags::all())
        .blend_enable(false);
    let blend_attachments = &[blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(blend_attachments);

    // Viewport and scissor are set when drawing, so a window resize doesn't need a new pipeline
    let dynamic_states = &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(dynamic_states);

    // Dynamic rendering: the formats of the images this pipeline will draw into
    let formats = &[format];
    let mut rendering = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(formats)
        .depth_attachment_format(if with_depth {
            DEPTH_FORMAT
        } else {
            vk::Format::UNDEFINED // no depth attachment
        });

    // Push constant: one 4×4 matrix (64 bytes) for the vertex shader
    let push = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(size_of::<PushConstants>() as u32);

    let layout = unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                // Set 0: texture
                .set_layouts(&[set_layout])
                .push_constant_ranges(&[push]),
            None,
        )?
    };

    // The big form: plug in every small form prepared above
    let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut rendering)
        .stages(stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .depth_stencil_state(&depth_stencil)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic)
        .layout(layout);

    // Create it (the function can make several pipelines, so it takes a list and returns a list)
    let pipeline = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)?
    }
    .0[0];

    Ok((layout, pipeline))
}
