//! Vulkan instance: the connection to Vulkan, plus the validation messages.

use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::ffi::{CStr, c_void};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use vulkanalia::window as vk_window;
use winit::window::Window;

use super::{PORTABILITY_MACOS_VERSION, VALIDATION_ENABLED, VALIDATION_LAYER};

/// Asks Vulkan to send warnings and errors to `debug_callback`.
pub(super) fn create_messenger(instance: &Instance) -> Result<vk::DebugUtilsMessengerEXT> {
    let info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .user_callback(Some(debug_callback));

    Ok(unsafe { instance.create_debug_utils_messenger_ext(&info, None)? })
}

/// Creates the instance: our connection to Vulkan, with the extensions and layers we need.
pub(super) fn create_instance(window: &Window, entry: &Entry) -> Result<Instance> {
    // 1. Who we are
    let application_info = vk::ApplicationInfo::builder()
        .application_name(b"Gravitas\0")
        .application_version(vk::make_version(0, 1, 0))
        .engine_name(b"No Engine\0")
        .engine_version(vk::make_version(1, 0, 0))
        .api_version(vk::make_version(1, 3, 0));

    // 2. Extensions needed to draw into a window on this OS
    let mut extensions = vk_window::get_required_instance_extensions(window)
        .iter()
        .map(|e| e.as_ptr())
        .collect::<Vec<_>>();

    if VALIDATION_ENABLED {
        extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
    }

    // 3. macOS only: accept MoltenVK (Vulkan running on top of Metal)
    let flags = if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        extensions.push(
            vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION
                .name
                .as_ptr(),
        );
        extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };

    // Validation layer: check it's installed before asking for it
    let available_layers = unsafe { entry.enumerate_instance_layer_properties()? }
        .iter()
        .map(|l| l.layer_name)
        .collect::<HashSet<_>>();

    if VALIDATION_ENABLED && !available_layers.contains(&VALIDATION_LAYER) {
        return Err(anyhow!("Validation layer requested but not available."));
    }

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    // 4. Create the instance
    let info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions)
        .flags(flags);

    Ok(unsafe { entry.create_instance(&info, None)? })
}

/// Called by the validation layer each time it has something to say. Prints it with a coloured marker.
///
/// `extern "system"`: uses the C calling convention, because Vulkan (written in C) calls it.
pub(super) extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _type: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let message = unsafe { CStr::from_ptr((*data).message) }.to_string_lossy();

    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        println!("🔴 VULKAN ERROR: {message}");
    } else {
        println!("🟡 VULKAN WARNING: {message}");
    }

    // FALSE = don't abort the Vulkan call that triggered the message
    vk::FALSE
}
