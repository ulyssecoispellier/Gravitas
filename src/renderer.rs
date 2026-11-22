//! The Vulkan renderer: sets up the GPU once, then draws a frame each time it's asked.
//!
//! Setup order (in `Renderer::new`): instance → debug messenger → surface → GPU → device
//! → swapchain → depth image → command buffer → sync objects → pipeline → mesh buffers.
//! `Drop` destroys everything in reverse order.

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

use crate::mesh::{Vertex, uv_sphere};

/// From this Vulkan version on, macOS needs the "portability" extensions to see MoltenVK / KosmicKrisp
const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
/// Pixel format of the depth image: one 32-bit float per pixel
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
/// Validation layers on in debug builds (`cargo run`), off in release (`cargo run --release`)
const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
/// The Khronos validation layer: checks every Vulkan call and reports mistakes
const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

/// The data sent to the shaders for each body drawn (through push constants).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PushConstants {
    /// Where to draw it on screen: projection × view × model (what you send today)
    pub transform: [f32; 16],
    /// Where this body is in space (x, y, z), + 4th number: 1.0 if the body glows (the star), else 0.0
    pub body_pos: [f32; 4],
    /// Where the star is (x, y, z), + an unused 4th number (padding)
    pub light_pos: [f32; 4],
}

/// Owns every Vulkan object. Created once by `new`, cleaned up by `Drop`.
pub struct Renderer {
    /// The loaded Vulkan library (kept alive; `_` because we never read it again)
    _entry: Entry,
    /// Our connection to Vulkan
    instance: Instance,
    /// Sends validation messages to `debug_callback`
    messenger: vk::DebugUtilsMessengerEXT,
    /// The real GPU we picked
    physical_device: vk::PhysicalDevice,
    /// Our session on the GPU (the "logical device"): almost every call goes through it
    device: Device,
    /// Where we submit command buffers for the GPU to run
    graphics_queue: vk::Queue,
    /// The window, as Vulkan sees it
    surface: vk::SurfaceKHR,
    /// The images shown in the window, in turn
    swapchain: Swapchain,
    /// Memory pool that command buffers are allocated from
    command_pool: vk::CommandPool,
    /// The list of commands, recorded again each frame
    command_buffer: vk::CommandBuffer,
    /// GPU signal: "the swapchain image is free to be drawn into"
    image_available: vk::Semaphore,
    /// GPU signal, one per swapchain image: "drawing is done, it can be presented"
    render_finished: Vec<vk::Semaphore>,
    /// CPU signal: "the GPU finished the previous frame"
    in_flight: vk::Fence,
    /// What extra data the shaders receive (here: one push constant matrix)
    pipeline_layout: vk::PipelineLayout,
    /// All the drawing settings, baked together (shaders, vertex format, depth test...)
    pipeline: vk::Pipeline,
    /// The sphere's vertices
    vertex_buffer: vk::Buffer,
    /// The memory behind `vertex_buffer`
    vertex_memory: vk::DeviceMemory,
    /// The sphere's indices (which vertices form each triangle)
    index_buffer: vk::Buffer,
    /// The memory behind `index_buffer`
    index_memory: vk::DeviceMemory,
    /// How many indices to draw per sphere
    index_count: u32,
    /// One depth per pixel, used to hide what's behind
    depth_image: vk::Image,
    /// The memory behind `depth_image` (GPU-only)
    depth_memory: vk::DeviceMemory,
    /// How the pipeline sees `depth_image`
    depth_view: vk::ImageView,
}

impl Renderer {
    /// Sets up everything needed to draw. Called once, when the window is created.
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

        // The depth image: same size as the swapchain images (one depth per pixel)
        let (depth_image, depth_memory, depth_view) =
            create_depth_image(&instance, &device, physical_device, swapchain.extent)?;

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

        // 6. The shaders (compiled to SPIR-V by build.rs, embedded in the program) and the pipeline
        let vert_module =
            create_shader_module(&device, include_bytes!("../shaders/triangle.vert.spv"))?;
        let frag_module =
            create_shader_module(&device, include_bytes!("../shaders/triangle.frag.spv"))?;

        let (pipeline_layout, pipeline) =
            create_pipeline(&device, swapchain.format, vert_module, frag_module)?;

        // 7. The sphere mesh, uploaded to the GPU (one mesh, drawn once per body)
        let (sphere_vertices, sphere_indices) = uv_sphere(32, 16, 0.7, [0.0, 0.0, 0.0]);
        let (vertex_buffer, vertex_memory) = create_buffer(
            &instance,
            &device,
            physical_device,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            &sphere_vertices,
        )?;
        let index_count = sphere_indices.len() as u32;

        let (index_buffer, index_memory) = create_buffer(
            &instance,
            &device,
            physical_device,
            vk::BufferUsageFlags::INDEX_BUFFER,
            &sphere_indices,
        )?;

        // The pipeline has its own copy of the shaders now, so the modules can go
        for module in [vert_module, frag_module] {
            unsafe {
                device.destroy_shader_module(module, None);
            }
        }

        // Debug info about the swapchain
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
            index_count,
            depth_image,
            depth_memory,
            depth_view,
        })
    }
}

impl Renderer {
    /// Draws one frame. Each body is drawn with its own transform matrix.
    ///
    /// Wait for the previous frame → get a swapchain image → record commands → submit → present.
    pub fn draw_frame(&mut self, pushes: &[PushConstants]) -> Result<()> {
        unsafe {
            // 1. Wait until the GPU has finished the previous frame, then re-arm the fence
            self.device
                .wait_for_fences(&[self.in_flight], true, u64::MAX)?;
            self.device.reset_fences(&[self.in_flight])?;

            // 2. Get the next swapchain image. The GPU signals `image_available` when it's really free
            let index = self
                .device
                .acquire_next_image_khr(
                    self.swapchain.handle,
                    u64::MAX,
                    self.image_available,
                    vk::Fence::null(),
                )?
                .0 as usize;

            // 3. Record this frame's commands
            self.device
                .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.record_commands(index, pushes)?;

            // 4. Submit: wait for the image before writing colours,
            // signal `render_finished` when done, then signal the fence
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

            // 5. Present: show the image once `render_finished` is signalled
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

    /// Records all the drawing commands for one frame into the command buffer.
    ///
    /// `i` = index of the swapchain image we draw into.
    fn record_commands(&self, i: usize, pushes: &[PushConstants]) -> Result<()> {
        unsafe {
            self.device.begin_command_buffer(
                self.command_buffer,
                &vk::CommandBufferBeginInfo::builder(),
            )?;

            // The whole colour image (1 mip level, 1 layer)
            let range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);

            // Barrier: switch the swapchain image to "ready to draw colours".
            // UNDEFINED = we don't care about its old content, it gets cleared anyway
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

            // Same for the depth image: make it ready for depth testing
            let depth_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                .level_count(1)
                .layer_count(1);

            let depth_barrier = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.depth_image)
                .subresource_range(depth_range);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[depth_barrier],
            );

            // Colour attachment: clear to black at the start, keep the result (it goes to the screen)
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

            // Depth attachment: clear to 1.0 (= farthest), so anything drawn is closer.
            // DONT_CARE: the depths are only needed during the frame, not after
            let depth_clear = vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            };

            let depth_attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(self.depth_view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(depth_clear);

            // Dynamic rendering: the images we draw into this time
            let rendering = vk::RenderingInfo::builder()
                .render_area(vk::Rect2D::builder().extent(self.swapchain.extent))
                .layer_count(1)
                .color_attachments(attachments)
                .depth_attachment(&depth_attachment);

            // Start drawing, and set what every draw uses: pipeline, mesh buffers, viewport
            self.device
                .cmd_begin_rendering(self.command_buffer, &rendering);
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
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
            // One draw per body: send its matrix (push constant), then draw the sphere
            for push in pushes {
                self.device.cmd_push_constants(
                    self.command_buffer,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::bytes_of(push),
                );
                self.device
                    .cmd_draw_indexed(self.command_buffer, self.index_count, 1, 0, 0, 0);
            }
            self.device.cmd_end_rendering(self.command_buffer);

            // Barrier: switch the image to "ready to be shown on screen"
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

    /// Width / height of the window, for the camera's projection.
    pub fn aspect(&self) -> f32 {
        self.swapchain.extent.width as f32 / self.swapchain.extent.height as f32
    }

    /// Destroys everything that depends on the window size: the swapchain, its image views
    /// and the depth image. Used by `Drop`, and when the window is resized.
    fn destroy_swapchain(&mut self) {
        unsafe {
            // Views first, then the images they look at, then the memory behind them
            for &img_view in &self.swapchain.image_views {
                self.device.destroy_image_view(img_view, None);
            }
            self.device.destroy_image_view(self.depth_view, None);
            self.device.destroy_image(self.depth_image, None);
            self.device.free_memory(self.depth_memory, None);
            // The swapchain last: it owns its images
            self.device
                .destroy_swapchain_khr(self.swapchain.handle, None);
        };
    }

    /// Rebuilds the swapchain and the depth image at the window's new size.
    /// The pipeline stays: its viewport and scissor are dynamic, so it doesn't depend on the size.
    pub fn recreate_swapchain(&mut self, window: &Window) -> Result<()> {
        unsafe {
            // Wait until the GPU stops using the old images, then destroy them
            self.device.device_wait_idle()?;
            self.destroy_swapchain();

            // New swapchain at the window's current size
            self.swapchain = create_swapchain(
                window,
                &self.instance,
                &self.device,
                self.physical_device,
                self.surface,
            )?;

            // New depth image, matching the new size (one depth per pixel)
            (self.depth_image, self.depth_memory, self.depth_view) = create_depth_image(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.extent,
            )?;
        }

        Ok(())
    }
}

/// Destroys every Vulkan object, in reverse order of creation (children before parents).
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            // Never destroy something the GPU might still be using
            self.device.device_wait_idle().unwrap();
            // Sync objects and commands
            self.device.destroy_semaphore(self.image_available, None);
            for &r_f in &self.render_finished {
                self.device.destroy_semaphore(r_f, None);
            }
            self.device.destroy_fence(self.in_flight, None);
            self.device.destroy_command_pool(self.command_pool, None);
            // Mesh buffers and their memory
            self.device.destroy_buffer(self.index_buffer, None);
            self.device.free_memory(self.index_memory, None);
            self.device.destroy_buffer(self.vertex_buffer, None);
            self.device.free_memory(self.vertex_memory, None);
            // Pipeline
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);
            // Window-size objects (swapchain, depth image), then the device itself
            self.destroy_swapchain();
            self.device.destroy_device(None);
            // Instance-level objects last
            self.instance.destroy_surface_khr(self.surface, None);
            if VALIDATION_ENABLED {
                self.instance
                    .destroy_debug_utils_messenger_ext(self.messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Asks Vulkan to send warnings and errors to `debug_callback`.
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

/// Creates the instance: our connection to Vulkan, with the extensions and layers we need.
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

    // FALSE = don't abort the Vulkan call that triggered the message
    vk::FALSE
}

/// Picks the best GPU that can both draw and show images in our window.
///
/// Returns the GPU and the queue family to use. "Best" = highest Vulkan version.
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
fn create_logical_device(
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

/// The images shown in the window, plus what we need to know about them.
struct Swapchain {
    /// The swapchain itself
    handle: vk::SwapchainKHR,
    /// Pixel format of the images
    format: vk::Format,
    /// Size of the images, in pixels
    extent: vk::Extent2D,
    /// The images, owned by the swapchain (we never destroy them ourselves)
    images: Vec<vk::Image>,
    /// One view per image, to use it as a colour attachment
    image_views: Vec<vk::ImageView>,
}

/// Creates the swapchain for the window: picks its pixel format, present mode, size and image count.
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

/// Wraps compiled SPIR-V bytes into a shader module Vulkan can use.
fn create_shader_module(device: &Device, bytes: &[u8]) -> Result<vk::ShaderModule> {
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
fn create_pipeline(
    device: &Device,
    format: vk::Format,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
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

    // Vertex format: comes from `Vertex` in mesh.rs
    let bindings = &[Vertex::binding_description()];
    let attributes = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(bindings)
        .vertex_attribute_descriptions(&attributes);

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

    // Depth test: keep a pixel only if it's closer (LESS) than what's stored, then store its depth
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS);

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
        .depth_attachment_format(DEPTH_FORMAT);

    // Push constant: one 4×4 matrix (64 bytes) for the vertex shader
    let push = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(size_of::<PushConstants>() as u32);

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

/// Creates a buffer and fills it with `data` (any `Copy` type: vertices, indices...).
///
/// Uses memory the CPU can write into directly (HOST_VISIBLE | HOST_COHERENT). Simple, but not the
/// fastest for the GPU: for big meshes, a copy to DEVICE_LOCAL memory ("staging buffer") is better.
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

/// Creates the depth image, its GPU-only memory and its view. Same size as the swapchain images.
fn create_depth_image(
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    extent: vk::Extent2D,
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
        .format(DEPTH_FORMAT)
        .tiling(vk::ImageTiling::OPTIMAL)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .samples(vk::SampleCountFlags::_1)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let image = unsafe { device.create_image(&info, None)? };

    let requirements = unsafe { device.get_image_memory_requirements(image) };

    // GPU-only memory: the fastest, and the CPU never needs to touch the depths
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
        .aspect_mask(vk::ImageAspectFlags::DEPTH) // ← DEPTH, not COLOR
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(DEPTH_FORMAT) // ← the depth format
        .subresource_range(range);
    let view = unsafe { device.create_image_view(&view_info, None)? };

    Ok((image, memory, view))
}
