//! The swapchain: the images shown in the window, in turn.

use anyhow::Result;
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::{KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands};
use winit::window::Window;

/// The images shown in the window, plus what we need to know about them.
pub(super) struct Swapchain {
    /// The swapchain itself
    pub(super) handle: vk::SwapchainKHR,
    /// Pixel format of the images
    pub(super) format: vk::Format,
    /// Size of the images, in pixels
    pub(super) extent: vk::Extent2D,
    /// The images, owned by the swapchain (we never destroy them ourselves)
    pub(super) images: Vec<vk::Image>,
    /// One view per image, to use it as a colour attachment
    pub(super) image_views: Vec<vk::ImageView>,
}

/// Creates the swapchain for the window: picks its pixel format, present mode, size and image count.
pub(super) fn create_swapchain(
    window: &Window,
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<Swapchain> {
    let capabilities =
        unsafe { instance.get_physical_device_surface_capabilities_khr(physical_device, surface)? };
    let formats =
        unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface)? };
    let present_modes = unsafe {
        instance.get_physical_device_surface_present_modes_khr(physical_device, surface)?
    };

    // Pixel format: prefer standard sRGB colours, otherwise take the first one available
    let format = formats
        .iter()
        .cloned()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or(formats[0]);

    // Present mode: prefer MAILBOX, otherwise FIFO (always available)
    let present_mode = present_modes
        .iter()
        .cloned()
        .find(|m| *m == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO);

    // Size: usually the window's. u32::MAX means "you choose", so clamp the window size to what's allowed
    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        let size = window.inner_size();
        vk::Extent2D {
            width: size.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: size.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    };

    // One more image than the minimum, so we rarely wait for the driver (max 0 = no maximum)
    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count != 0 && image_count > capabilities.max_image_count {
        image_count = capabilities.max_image_count;
    }

    let info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface) // draw into this window
        .min_image_count(image_count) // how many images
        .image_format(format.format) // pixel format
        .image_color_space(format.color_space)
        .image_extent(extent) // size in pixels
        .image_array_layers(1) // 1 layer (2 would be for VR)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT) // we'll draw colours into them
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE) // used by one queue family only
        .pre_transform(capabilities.current_transform) // no rotation/flip
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // window isn't see-through
        .present_mode(present_mode) // MAILBOX or FIFO
        .clipped(true); // skip pixels hidden by other windows

    let handle = unsafe { device.create_swapchain_khr(&info, None)? };
    let images = unsafe { device.get_swapchain_images_khr(handle)? };

    // One view per image: how the pipeline will see it
    let image_views = images
        .iter()
        .map(|&image| {
            let range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR) // it's a colour image
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::_2D) // read it as a flat 2D image
                .format(format.format)
                .subresource_range(range);
            unsafe { device.create_image_view(&info, None) }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Swapchain {
        handle,
        format: format.format,
        extent,
        images,
        image_views,
    })
}
