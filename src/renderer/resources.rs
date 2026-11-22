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

    // GPU-only memory: the fastest. The CPU never writes here directly (textures arrive by a GPU copy)
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

    // A view on the depth part of the image
    let range = vk::ImageSubresourceRange::builder()
        .aspect_mask(aspect) // COLOR for a texture, DEPTH for the depth image
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format) // read it in the same format it was created with
        .subresource_range(range);
    let view = unsafe { device.create_image_view(&view_info, None)? };

    Ok((image, memory, view))
}
