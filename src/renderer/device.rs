//! Choosing the GPU and opening a session on it (the logical device).

use anyhow::{Result, anyhow};
use std::collections::HashSet;
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

use super::{VALIDATION_ENABLED, VALIDATION_LAYER};

/// Picks the best GPU that can both draw and show images in our window.
///
/// Returns the GPU and the queue family to use. "Best" = highest Vulkan version.
pub(super) fn pick_physical_device(
    instance: &Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32)> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let mut best: Option<(vk::PhysicalDevice, u32, u32)> = None;
    for device in devices {
        let props = unsafe { instance.get_physical_device_properties(device) };
        println!(
            "GPU: {} | Vulkan {}.{}.{}",
            props.device_name,
            vk::version_major(props.api_version),
            vk::version_minor(props.api_version),
            vk::version_patch(props.api_version),
        );

        // Find a queue family that can draw AND present to our window
        let families = unsafe { instance.get_physical_device_queue_family_properties(device) };

        let mut graphics = None;
        for (i, family) in families.iter().enumerate() {
            let can_draw = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let can_present = unsafe {
                instance.get_physical_device_surface_support_khr(device, i as u32, surface)?
            };

            if can_draw && can_present {
                graphics = Some(i);
                break;
            }
        }

        // Keep this GPU if it beats the best one so far
        if let Some(index) = graphics {
            let is_better = match best {
                None => true,
                Some((_, _, best_version)) => props.api_version > best_version,
            };

            if is_better {
                best = Some((device, index as u32, props.api_version));
                println!("  → new best GPU");
            }
        }
    }

    match best {
        Some((device, family, _)) => Ok((device, family)),
        None => Err(anyhow!("No suitable GPU found.")),
    }
}

/// Opens a session on the GPU (the logical device) with the extensions and features we need,
/// and gets its queue.
pub(super) fn create_logical_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_family: u32,
) -> Result<(Device, vk::Queue)> {
    // One queue, from the graphics family
    let queue_priorities = &[1.0];
    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(graphics_family)
        .queue_priorities(queue_priorities);
    let queue_infos = &[queue_info];

    // Extensions: swapchain (to show images), + portability subset on macOS if the driver has it
    let available_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device, None)? }
            .iter()
            .map(|e| e.extension_name)
            .collect::<HashSet<_>>();

    let mut extensions = vec![vk::KHR_SWAPCHAIN_EXTENSION.name.as_ptr()];
    if available_extensions.contains(&vk::KHR_PORTABILITY_SUBSET_EXTENSION.name) {
        extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }

    // Same validation layer as the instance (needed by older drivers)
    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    // Optional GPU features. fill_mode_non_solid = wireframe (not supported by KosmicKrisp)
    let features = vk::PhysicalDeviceFeatures::builder().fill_mode_non_solid(false);

    // Vulkan 1.3 features: dynamic rendering (draw without render pass objects)
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::builder().dynamic_rendering(true);

    let info = vk::DeviceCreateInfo::builder()
        .push_next(&mut vulkan13)
        .queue_create_infos(queue_infos)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions)
        .enabled_features(&features);

    let device = unsafe { instance.create_device(physical_device, &info, None)? };
    let queue = unsafe { device.get_device_queue(graphics_family, 0) };

    Ok((device, queue))
}
