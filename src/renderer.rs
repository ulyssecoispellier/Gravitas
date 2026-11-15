use anyhow::{Result, anyhow};
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::window as vk_window;
use winit::window::Window;

const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);

pub struct Renderer {
    _entry: Entry,
    instance: Instance,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let _entry = unsafe { Entry::new(loader) }.map_err(|e| anyhow!("{}", e))?;
        let instance = create_instance(window, &_entry)?;

        println!("✅ Vulkan instance created");
        Ok(Self { _entry, instance })
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

fn create_instance(window: &Window, entry: &Entry) -> Result<Instance> {
    // 1. Who we are
    let application_info = vk::ApplicationInfo::builder()
        .application_name(b"Gravitas\0")
        .application_version(vk::make_version(0, 1, 0))
        .engine_name(b"No Engine\0")
        .engine_version(vk::make_version(1, 0, 0))
        .api_version(vk::make_version(1, 0, 0));

    // 2. Extensions needed to draw into a window on this OS
    let mut extensions = vk_window::get_required_instance_extensions(window)
        .iter()
        .map(|e| e.as_ptr())
        .collect::<Vec<_>>();

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

    // 4. Create the instance
    let info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_extension_names(&extensions)
        .flags(flags);

    Ok(unsafe { entry.create_instance(&info, None)? })
}
