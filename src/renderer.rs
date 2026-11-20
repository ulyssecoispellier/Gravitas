use std::collections::HashSet;

use anyhow::{Ok, Result, anyhow};
use std::ffi::{CStr, c_void};
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::{
    ExtDebugUtilsExtensionInstanceCommands, KhrSurfaceExtensionInstanceCommands,
    KhrSwapchainExtensionDeviceCommands,
};
use vulkanalia::window as vk_window;
use winit::window::Window;

use crate::mesh::{SQUARE, SQUARE_INDICES, Vertex};

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
    device: Device,
    graphics_queue: vk::Queue,
    surface: vk::SurfaceKHR,
    swapchain: Swapchain,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: Vec<vk::Semaphore>,
    in_flight: vk::Fence,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        // 1. Connect to Vulkan
        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let _entry = unsafe { Entry::new(loader) }.map_err(|e| anyhow!("{}", e))?;
        let instance = create_instance(window, &_entry)?;
        let messenger = if VALIDATION_ENABLED {
            create_messenger(&instance)?
        } else {
            vk::DebugUtilsMessengerEXT::null()
        };

        // 2. Link Vulkan to the window, pick a GPU, open a session on it
        let surface = unsafe { vk_window::create_surface(&instance, &window, &window)? };
        let (physical_device, graphics_family) = pick_physical_device(&instance, surface)?;
        let (device, graphics_queue) =
            create_logical_device(&instance, physical_device, graphics_family)?;

        // 3. The images that will be shown in the window
        let swapchain = create_swapchain(window, &instance, &device, physical_device, surface)?;

        // 4. Where we record drawing commands
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(graphics_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None)? };

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe { device.allocate_command_buffers(&alloc_info)? }[0];

        // 5. Synchronization: GPU↔GPU (semaphores) and GPU→CPU (fence)
        let image_available =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::builder(), None)? };
        let render_finished = (0..swapchain.images.len())
            .map(|_| unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::builder(), None) })
            .collect::<Result<Vec<_>, _>>()?;
        let in_flight = unsafe {
            device.create_fence(
                &vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?
        };

        let vert_module =
            create_shader_module(&device, include_bytes!("../shaders/triangle.vert.spv"))?;
        let frag_module =
            create_shader_module(&device, include_bytes!("../shaders/triangle.frag.spv"))?;

        let (pipeline_layout, pipeline) =
            create_pipeline(&device, swapchain.format, vert_module, frag_module)?;

        let (vertex_buffer, vertex_memory) = create_buffer(
            &instance,
            &device,
            physical_device,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            &SQUARE,
        )?;

        let (index_buffer, index_memory) = create_buffer(
            &instance,
            &device,
            physical_device,
            vk::BufferUsageFlags::INDEX_BUFFER,
            &SQUARE_INDICES,
        )?;

        for module in [vert_module, frag_module] {
            unsafe {
                device.destroy_shader_module(module, None);
            }
        }

        println!(
            "Nb of img : {:?} | Width : {:?} | Height {:?} | Format : {:?}",
            swapchain.images.len(),
            swapchain.extent.width,
            swapchain.extent.height,
            swapchain.format,
        );

        Ok(Self {
            _entry,
            instance,
            messenger,
            physical_device,
            graphics_family,
            device,
            graphics_queue,
            surface,
            swapchain,
            command_pool,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
            pipeline_layout,
            pipeline,
            vertex_buffer,
            vertex_memory,
            index_buffer,
            index_memory,
        })
    }
}

impl Renderer {
    pub fn draw_frame(&mut self, offset: [f32; 2]) -> Result<()> {
        unsafe {
            self.device
                .wait_for_fences(&[self.in_flight], true, u64::MAX)?;
            self.device.reset_fences(&[self.in_flight])?;

            let index = self
                .device
                .acquire_next_image_khr(
                    self.swapchain.handle,
                    u64::MAX,
                    self.image_available,
                    vk::Fence::null(),
                )?
                .0 as usize;

            self.device
                .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.record_commands(index, offset)?;

            let wait_semaphores = &[self.image_available];
            let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let command_buffers = &[self.command_buffer];
            let signal_semaphores = &[self.render_finished[index]];

            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(wait_semaphores)
                .wait_dst_stage_mask(wait_stages)
                .command_buffers(command_buffers)
                .signal_semaphores(signal_semaphores);

            self.device
                .queue_submit(self.graphics_queue, &[submit_info], self.in_flight)?;

            let swapchains = &[self.swapchain.handle];
            let image_indices = &[index as u32];

            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(signal_semaphores)
                .swapchains(swapchains)
                .image_indices(image_indices);

            self.device
                .queue_present_khr(self.graphics_queue, &present_info)?;
        }

        Ok(())
    }

    fn record_commands(&self, i: usize, offset: [f32; 2]) -> Result<()> {
        unsafe {
            self.device.begin_command_buffer(
                self.command_buffer,
                &vk::CommandBufferBeginInfo::builder(),
            )?;

            let range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);

            let to_draw = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.swapchain.images[i])
                .subresource_range(range);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, // wait for this stage...
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, // ...before this stage
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[to_draw],
            );

            let clear = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            };

            let attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(self.swapchain.image_views[i])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear);
            let attachments = &[attachment];

            let rendering = vk::RenderingInfo::builder()
                .render_area(vk::Rect2D::builder().extent(self.swapchain.extent))
                .layer_count(1)
                .color_attachments(attachments);

            self.device
                .cmd_begin_rendering(self.command_buffer, &rendering);
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            self.device.cmd_push_constants(
                self.command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::bytes_of(&offset),
            );
            self.device.cmd_bind_vertex_buffers(
                self.command_buffer,
                0,
                &[self.vertex_buffer],
                &[0],
            );
            self.device.cmd_bind_index_buffer(
                self.command_buffer,
                self.index_buffer,
                0,
                vk::IndexType::UINT16,
            );
            self.device.cmd_set_viewport(
                self.command_buffer,
                0,
                &[vk::Viewport::builder()
                    .width(self.swapchain.extent.width as f32)
                    .height(self.swapchain.extent.height as f32)
                    .max_depth(1.0)],
            );
            self.device.cmd_set_scissor(
                self.command_buffer,
                0,
                &[vk::Rect2D::builder().extent(self.swapchain.extent)],
            );
            self.device.cmd_draw_indexed(
                self.command_buffer,
                SQUARE_INDICES.len() as u32,
                1,
                0,
                0,
                0,
            );
            self.device.cmd_end_rendering(self.command_buffer);

            let to_present = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::empty())
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.swapchain.images[i])
                .subresource_range(range);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[to_present],
            );

            self.device.end_command_buffer(self.command_buffer)?;
        }

        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_semaphore(self.image_available, None);
            for &r_f in &self.render_finished {
                self.device.destroy_semaphore(r_f, None);
            }
            self.device.destroy_fence(self.in_flight, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_buffer(self.index_buffer, None);
            self.device.free_memory(self.index_memory, None);
            self.device.destroy_buffer(self.vertex_buffer, None);
            self.device.free_memory(self.vertex_memory, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);
            for &img_view in &self.swapchain.image_views {
                self.device.destroy_image_view(img_view, None);
            }
            self.device
                .destroy_swapchain_khr(self.swapchain.handle, None);
            self.device.destroy_device(None);
            self.instance.destroy_surface_khr(self.surface, None);
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

fn pick_physical_device(
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

    let available_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device, None)? }
            .iter()
            .map(|e| e.extension_name)
            .collect::<HashSet<_>>();

    let mut extensions = vec![vk::KHR_SWAPCHAIN_EXTENSION.name.as_ptr()];
    if available_extensions.contains(&vk::KHR_PORTABILITY_SUBSET_EXTENSION.name) {
        extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    let features = vk::PhysicalDeviceFeatures::builder();

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

struct Swapchain {
    handle: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
}

fn create_swapchain(
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

    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count != 0 && image_count > capabilities.max_image_count {
        image_count = capabilities.max_image_count;
    }

    let info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface) // draw into this window
        .min_image_count(image_count) // how many images (step 5)
        .image_format(format.format) // pixel format (step 3)
        .image_color_space(format.color_space)
        .image_extent(extent) // size in pixels (step 4)
        .image_array_layers(1) // 1 layer (2 would be for VR)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT) // we'll draw colours into them
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE) // used by one queue family only
        .pre_transform(capabilities.current_transform) // no rotation/flip
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // window isn't see-through
        .present_mode(present_mode) // MAILBOX or FIFO (step 3)
        .clipped(true); // skip pixels hidden by other windows

    let handle = unsafe { device.create_swapchain_khr(&info, None)? };
    let images = unsafe { device.get_swapchain_images_khr(handle)? };

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

fn create_shader_module(device: &Device, bytes: &[u8]) -> Result<vk::ShaderModule> {
    let bytecode = vulkanalia::bytecode::Bytecode::new(bytes)?;
    let info = vk::ShaderModuleCreateInfo::builder()
        .code_size(bytecode.code_size())
        .code(bytecode.code());

    Ok(unsafe { device.create_shader_module(&info, None)? })
}

fn create_pipeline(
    device: &Device,
    format: vk::Format,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
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

    let bindings = &[Vertex::binding_description()];
    let attributes = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(bindings)
        .vertex_attribute_descriptions(&attributes);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1);

    // Filled triangles, draw both faces
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::CLOCKWISE);

    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::_1);

    let blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(vk::ColorComponentFlags::all())
        .blend_enable(false);
    let blend_attachments = &[blend_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(blend_attachments);

    let dynamic_states = &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(dynamic_states);

    let formats = &[format];
    let mut rendering =
        vk::PipelineRenderingCreateInfo::builder().color_attachment_formats(formats);

    let push = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(std::mem::size_of::<[f32; 2]>() as u32);

    let layout = unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&[push]),
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

fn create_buffer<T: Copy>(
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

    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;

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

    unsafe {
        device.bind_buffer_memory(buffer, memory, 0)?;
        let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast(), data.len());
        device.unmap_memory(memory);
    };

    Ok((buffer, memory))
}
