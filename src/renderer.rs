use std::collections::HashSet;

use anyhow::{Result, anyhow};
use std::ffi::{CStr, c_void};
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use vulkanalia::window as vk_window;
use winit::window::Window;

const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

pub struct Renderer {
    _entry: Entry,
    instance: Instance,
    messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_family: u32,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let _entry = unsafe { Entry::new(loader) }.map_err(|e| anyhow!("{}", e))?;
        let instance = create_instance(window, &_entry)?;
        let messenger = if VALIDATION_ENABLED {
            create_messenger(&instance)?
        } else {
            vk::DebugUtilsMessengerEXT::null()
        };

        let (physical_device, graphics_family) = pick_physical_device(&instance)?;

        println!("✅ Vulkan instance created");
        Ok(Self {
            _entry,
            instance,
            messenger,
            physical_device,
            graphics_family,
        })
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            if VALIDATION_ENABLED {
                self.instance
                    .destroy_debug_utils_messenger_ext(self.messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

fn create_messenger(instance: &Instance) -> Result<vk::DebugUtilsMessengerEXT> {
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

fn create_instance(window: &Window, entry: &Entry) -> Result<Instance> {
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

extern "system" fn debug_callback(
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

    vk::FALSE
}

fn pick_physical_device(instance: &Instance) -> Result<(vk::PhysicalDevice, u32)> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let mut best: Option<(vk::PhysicalDevice, u32, u32)> = None;
    for device in devices {
        let props = unsafe { instance.get_physical_device_properties(device) };
        println!(
            "GPU: {} | Vulkan {}.{}",
            props.device_name,
            vk::version_major(props.api_version),
            vk::version_minor(props.api_version),
        );

        let families = unsafe { instance.get_physical_device_queue_family_properties(device) };

        let graphics = families
            .iter()
            .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS));

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

fn create_logical_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_family: u32,
) -> Result<(Device, vk::Queue)> {
    let queue_priorities = &[1.0];
    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(graphics_family)
        .queue_priorities(queue_priorities);
    let queue_infos = &[queue_info];

    let available_extensions = unsafe {
        instance.enumerate_device_extension_properties(physical_device, None)?
    }
    .iter()
    .map(|e| e.extension_name)
    .collect::<HashSet<_>>();
    
    let mut extensions = Vec::new();
    if available_extensions.contains(&vk::KHR_PORTABILITY_SUBSET_EXTENSION.name) {
        extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    let features = vk::PhysicalDeviceFeatures::builder();

    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(queue_infos)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions)
        .enabled_features(&features);

    let device = unsafe { instance.create_device(physical_device, &info, None)? };
    let queue = unsafe { device.get_device_queue(graphics_family, 0) };
    
    Ok((device, queue))
}
