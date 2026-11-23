//! GPU memory: buffers and images, and finding the right kind of memory for them.

use anyhow::{Result, anyhow};
use vulkanalia::prelude::v1_3::*;

/// Creates a buffer and fills it with `data` (any `Copy` type: vertices, indices...).
///
/// Uses memory the CPU can write into directly (HOST_VISIBLE | HOST_COHERENT). Simple, but not the
/// fastest for the GPU: for big meshes, a copy to DEVICE_LOCAL memory ("staging buffer") is better.
pub(super) fn create_buffer<T: Copy>(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    usage: vk::BufferUsageFlags,
    data: &[T],
) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let size = (std::mem::size_of::<T>() * data.len()) as u64;

    let buffer_info = vk::BufferCreateInfo::builder()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe { device.create_buffer(&buffer_info, None)? };

    // How much memory it needs, and which memory types are allowed
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    // Memory the CPU can write to, with no manual flushing
    let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;

    // Find a memory type that is allowed (bit i is set) AND has the flags we want
    let memory_type_index = (0..memory_properties.memory_type_count)
        .find(|&i| {
            let allowed = (requirements.memory_type_bits & (1 << i)) != 0;
            let has_flags = memory_properties.memory_types[i as usize]
                .property_flags
                .contains(wanted);
            allowed && has_flags
        })
        .ok_or_else(|| anyhow!("no suitable memory type"))?;

    let info = vk::MemoryAllocateInfo::builder()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);

    let memory = unsafe { device.allocate_memory(&info, None)? };

    // Attach the memory, then copy the data into it through a CPU pointer
    unsafe {
        device.bind_buffer_memory(buffer, memory, 0)?;
        let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast(), data.len());
        device.unmap_memory(memory);
    };

    Ok((buffer, memory))
}

/// Creates a 2D image, its GPU-only memory and a view on it.
///
/// Used for the depth image and for textures: only the format, the usage and the aspect change.
pub(super) fn create_image(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    extent: vk::Extent2D,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    aspect: vk::ImageAspectFlags,
) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView)> {
    let info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .format(format)
        .tiling(vk::ImageTiling::OPTIMAL)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(usage)
        .samples(vk::SampleCountFlags::_1)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let image = unsafe { device.create_image(&info, None)? };

    let requirements = unsafe { device.get_image_memory_requirements(image) };

    // GPU-only memory
    let wanted = vk::MemoryPropertyFlags::DEVICE_LOCAL;

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    // Same search as in `create_buffer`
    let memory_type_index = (0..memory_properties.memory_type_count)
        .find(|&i| {
            let allowed = (requirements.memory_type_bits & (1 << i)) != 0;
            let has_flags = memory_properties.memory_types[i as usize]
                .property_flags
                .contains(wanted);
            allowed && has_flags
        })
        .ok_or_else(|| anyhow!("no suitable memory type"))?;

    let alloc_info = vk::MemoryAllocateInfo::builder()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);
    let memory = unsafe { device.allocate_memory(&alloc_info, None)? };

    unsafe { device.bind_image_memory(image, memory, 0)? };

    // View on the whole image
    let range = vk::ImageSubresourceRange::builder()
        .aspect_mask(aspect)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format)
        .subresource_range(range);
    let view = unsafe { device.create_image_view(&view_info, None)? };

    Ok((image, memory, view))
}

/// Copies `buffer` into `image` (one-time command buffer), then transitions it to SHADER_READ_ONLY_OPTIMAL.
pub(super) fn copy_buffer_to_image(
    device: &Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    buffer: vk::Buffer,
    image: vk::Image,
    width: u32,
    height: u32,
) -> Result<()> {
    unsafe {
        // 1. Allocate a temporary command buffer
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd = device.allocate_command_buffers(&alloc_info)?[0];

        // 2. Record (submitted once)
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device.begin_command_buffer(cmd, &begin_info)?;

        let range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);

        // UNDEFINED → TRANSFER_DST_OPTIMAL
        let to_transfer = vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[] as &[vk::MemoryBarrier],
            &[] as &[vk::BufferMemoryBarrier],
            &[to_transfer],
        );

        // Buffer → image, full size
        let region = vk::BufferImageCopy::builder()
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });
        device.cmd_copy_buffer_to_image(
            cmd,
            buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );

        // TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL (for the fragment shader)
        let to_shader = vk::ImageMemoryBarrier::builder()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[] as &[vk::MemoryBarrier],
            &[] as &[vk::BufferMemoryBarrier],
            &[to_shader],
        );

        // 3. Stop recording
        device.end_command_buffer(cmd)?;

        // 4. Submit (no semaphores, no fence)
        let command_buffers = &[cmd];
        let submit_info = vk::SubmitInfo::builder().command_buffers(command_buffers);
        device.queue_submit(queue, &[submit_info], vk::Fence::null())?;

        // 5. Wait for the copy to finish
        device.queue_wait_idle(queue)?;

        // 6. Free the command buffer
        device.free_command_buffers(pool, command_buffers);
    }

    Ok(())
}

/// Points binding `binding` of `set` at `view`, read through `sampler` (image in SHADER_READ_ONLY_OPTIMAL).
pub(super) fn write_image_descriptor(
    device: &Device,
    set: vk::DescriptorSet,
    binding: u32,
    sampler: vk::Sampler,
    view: vk::ImageView,
) {
    let image_info = vk::DescriptorImageInfo::builder()
        .sampler(sampler)
        .image_view(view)
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let image_infos = &[image_info];
    let write = vk::WriteDescriptorSet::builder()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(image_infos);
    unsafe { device.update_descriptor_sets(&[write], &[] as &[vk::CopyDescriptorSet]) };
}

/// Uploads `pixels` into a new GPU-only texture: image + staging buffer + copy, then frees the staging buffer.
/// Returns the image, its memory and its view, ready to be sampled by shaders.
pub(super) fn create_texture<T: Copy>(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    queue: vk::Queue,
    pool: vk::CommandPool,
    pixels: &[T],
    width: u32,
    height: u32,
    format: vk::Format,
) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView)> {
    // Texture image (copy destination, sampled by shaders)
    let (texture_image, texture_memory, texture_view) = create_image(
        instance,
        device,
        physical_device,
        vk::Extent2D { width, height },
        format,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::ImageAspectFlags::COLOR,
    )?;

    // Staging buffer (CPU-writable copy source)
    let (staging_buffer, staging_memory) = create_buffer(
        instance,
        device,
        physical_device,
        vk::BufferUsageFlags::TRANSFER_SRC,
        pixels,
    )?;

    // Upload: staging buffer → texture image
    copy_buffer_to_image(
        device,
        queue,
        pool,
        staging_buffer,
        texture_image,
        width,
        height,
    )?;

    // Staging buffer no longer needed
    unsafe {
        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_memory, None);
    }

    Ok((texture_image, texture_memory, texture_view))
}
