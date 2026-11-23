//! The Vulkan renderer: sets up the GPU once, then draws a frame each time it's asked.
//!
//! Setup order (in `Renderer::new`): instance → debug messenger → surface → GPU → device
//! → swapchain → depth image → command buffer → sync objects → pipeline → mesh buffers.
//! `Drop` destroys everything in reverse order.
//!
//! This file tells the story (setup, drawing a frame, cleanup). The helpers live in sub-files:
//! `instance`, `device`, `swapchain`, `pipeline` and `resources` (buffers and images).

use anyhow::{Ok, Result, anyhow};
use bytemuck::Zeroable;
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::{
    ExtDebugUtilsExtensionInstanceCommands, KhrSurfaceExtensionInstanceCommands,
    KhrSwapchainExtensionDeviceCommands,
};
use vulkanalia::window as vk_window;
use winit::window::Window;

use crate::mesh::uv_sphere;

mod device;
mod instance;
mod pipeline;
mod resources;
mod swapchain;

use device::{create_logical_device, pick_physical_device};
use instance::{create_instance, create_messenger};
use pipeline::{create_pipeline, create_shader_module};
use resources::{create_buffer, create_image, create_texture, write_image_descriptor};
use swapchain::{Swapchain, create_swapchain};

/// From this Vulkan version on, macOS needs the "portability" extensions to see MoltenVK / KosmicKrisp
const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
/// Pixel format of the depth image: one 32-bit float per pixel
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
/// Validation layers on in debug builds (`cargo run`), off in release (`cargo run --release`)
const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
/// The Khronos validation layer: checks every Vulkan call and reports mistakes
const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");
/// Max number of bodies the shaders know about (for shadows)
pub const MAX_BODIES: usize = 16;
/// Format of the scene image: 16-bit floats per channel, so light can go above 1.0 (HDR)
const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// Albedo maps, one texture-array layer each, in GPU body-slot order (layer = slot − 1)
const BODY_TEXTURES: [&str; 15] = [
    "textures/mercury/albedo.jpg",
    "textures/venus/albedo.jpg",
    "textures/earth/albedo.jpg",
    "textures/mars/albedo.jpg",
    "textures/jupiter/albedo.jpg",
    "textures/saturn/albedo.jpg",
    "textures/uranus/albedo.jpg",
    "textures/neptune/albedo.jpg",
    "textures/earth/satellites/moon/albedo.jpg",
    "textures/jupiter/satellites/io/albedo.jpg",
    "textures/jupiter/satellites/europa/albedo.jpg",
    "textures/jupiter/satellites/ganymede/albedo.jpg",
    "textures/jupiter/satellites/callisto/albedo.jpg",
    "textures/saturn/satellites/titan/albedo.jpg",
    "textures/neptune/satellites/triton/albedo.jpg",
];

/// The data sent to the shaders for each body drawn (through push constants).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PushConstants {
    /// projection × view × model
    pub transform: [f32; 16],
    /// Where this body is in space (x, y, z), w = unused
    pub body_pos: [f32; 4],
    /// Where the Sun is (x, y, z), w = unused
    pub light_pos: [f32; 4],
    /// x = radius, y = glow flag, z = texture flag, w = unused
    pub info: [f32; 4],
    /// Base colour (r, g, b), w = unused
    pub colour: [f32; 4],
}

/// The data sent to the ray-tracing shader (through push constants).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RayPushConstants {
    /// projection × view: world → screen (the shader inverts it)
    pub view_proj: [f32; 16],
    /// Camera position (x, y, z), w = frame number (per-frame randomness)
    pub camera_pos: [f32; 4],
    /// Sun: centre (x, y, z), w = radius
    pub sun: [f32; 4],
}

/// The data sent to the post-processing shader (through push constants).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostPushConstants {
    /// x = retro dithering on (1.0) / off (0.0), y z w = unused
    pub settings: [f32; 4],
}

/// Every body's data for the shaders (uniform buffer, binding 1): positions + radii, and colours.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BodiesUniform {
    pub bodies: [[f32; 4]; MAX_BODIES],
    pub colours: [[f32; 4]; MAX_BODIES],
    /// rgb = scattering strength per colour (β, per planet radius), w = outer radius ÷ planet radius (0 = none)
    pub atmospheres: [[f32; 4]; MAX_BODIES],
    /// rgb = Mie scattering strength per colour (β), w = forward bias g
    pub haze: [[f32; 4]; MAX_BODIES],
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
    /// Layout of the ray-tracing pass (same data as `pipeline_layout`)
    ray_pipeline_layout: vk::PipelineLayout,
    /// Full-screen ray-tracing pass (no vertex buffer)
    ray_pipeline: vk::Pipeline,
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
    /// The scene is drawn here, then the post pass reads it
    scene_image: vk::Image,
    /// The memory behind `scene_image`
    scene_memory: vk::DeviceMemory,
    /// How shaders see `scene_image`
    scene_view: vk::ImageView,
    /// Layout of the post-processing pass
    post_pipeline_layout: vk::PipelineLayout,
    /// Post-processing pass: reads `scene_image`, writes the swapchain image
    post_pipeline: vk::Pipeline,
    /// Set 0 of the post pass: the scene image
    post_descriptor_set: vk::DescriptorSet,
    /// Earth's texture (pixels from textures/earth/albedo.jpg)
    texture_image: vk::Image,
    /// The memory behind `texture_image` (GPU-only)
    texture_memory: vk::DeviceMemory,
    /// How shaders see `texture_image`
    texture_view: vk::ImageView,
    sky_image: vk::Image,
    sky_memory: vk::DeviceMemory,
    sky_view: vk::ImageView,
    /// Sampler for the Earth and sky textures: u wraps (longitude), v clamped (poles)
    sampler: vk::Sampler,
    /// Sampler for the scene image in the post pass: clamps on every edge (the screen does not wrap)
    post_sampler: vk::Sampler,
    /// Layout of set 0: binding 0 = texture + sampler
    descriptor_set_layout: vk::DescriptorSetLayout,
    /// Pool for `descriptor_set`
    descriptor_pool: vk::DescriptorPool,
    /// Set 0: Earth texture (0), bodies (1), sky (2)
    descriptor_set: vk::DescriptorSet,
    /// Every body's position + radius (uniform buffer, read by the planets' shader)
    bodies_buffer: vk::Buffer,
    /// The memory behind `bodies_buffer` (CPU-writable, updated every frame)
    bodies_memory: vk::DeviceMemory,
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
        let (depth_image, depth_memory, depth_view) = create_image(
            &instance,
            &device,
            physical_device,
            swapchain.extent,
            DEPTH_FORMAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
            1,
        )?;

        // Offscreen image: the scene is drawn here, then read by the post pass
        let (scene_image, scene_memory, scene_view) = create_image(
            &instance,
            &device,
            physical_device,
            swapchain.extent,
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
            1,
        )?;

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

        // Descriptor set layout: binding 0 = texture + sampler, fragment shader
        let sampler_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        // Binding 1: every body's position + radius (uniform buffer), fragment shader
        let bodies_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        // Binding 2: the sky texture (equirectangular star map), fragment shader
        let sky_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(2)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let descriptor_set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&[
                    sampler_binding,
                    bodies_binding,
                    sky_binding,
                ]),
                None,
            )?
        };

        // Ray-tracing pass: full-screen triangle, no vertex buffer
        let ray_vert_module =
            create_shader_module(&device, include_bytes!("../shaders/raytrace.vert.spv"))?;
        let ray_frag_module =
            create_shader_module(&device, include_bytes!("../shaders/raytrace.frag.spv"))?;

        let (ray_pipeline_layout, ray_pipeline) = create_pipeline(
            &device,
            HDR_FORMAT,
            ray_vert_module,
            ray_frag_module,
            descriptor_set_layout,
            false,
            true,
        )?;

        // Post-processing pass: same full-screen vertex shader, no vertex buffer, no depth
        let post_frag_module =
            create_shader_module(&device, include_bytes!("../shaders/post.frag.spv"))?;
        let (post_pipeline_layout, post_pipeline) = create_pipeline(
            &device,
            swapchain.format,
            ray_vert_module,
            post_frag_module,
            descriptor_set_layout,
            false,
            false,
        )?;

        // Mesh pass: the planets
        let (pipeline_layout, pipeline) = create_pipeline(
            &device,
            HDR_FORMAT,
            vert_module,
            frag_module,
            descriptor_set_layout,
            true,
            true,
        )?;

        /* // 7. Earth texture (Solar System Scope 8k day map): load as RGBA8
        let img = image::open("textures/earth/albedo.jpg")?.to_rgba8();
        let (tex_width, tex_height) = img.dimensions();
        let pixels = img.into_raw(); */

        let mut pixels: Vec<u8> = Vec::new();
        for path in BODY_TEXTURES {
            let img = image::open(path)?.to_rgba8();
            pixels.extend(img.into_raw());
        }

        // Uniform buffer: every body's position + radius, read by the planets' shader (shadows)
        let (bodies_buffer, bodies_memory) = create_buffer(
            &instance,
            &device,
            physical_device,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            &[BodiesUniform::zeroed()],
        )?;

        let (texture_image, texture_memory, texture_view) = create_texture(
            &instance,
            &device,
            physical_device,
            graphics_queue,
            command_pool,
            &pixels,
            4096,
            2048,
            BODY_TEXTURES.len() as u32,
            vk::Format::R8G8B8A8_SRGB,
        )?;

        let sky = image::open("textures/sky.exr")?.to_rgba32f();
        let (sky_width, sky_height) = sky.dimensions();
        let sky_floats = sky.into_raw();

        let sky_pixels: Vec<half::f16> =
            sky_floats.iter().map(|x| half::f16::from_f32(*x)).collect();

        let (sky_image, sky_memory, sky_view) = create_texture(
            &instance,
            &device,
            physical_device,
            graphics_queue,
            command_pool,
            &sky_pixels,
            sky_width,
            sky_height,
            1,
            HDR_FORMAT,
        )?;

        // Sampler: linear filtering, u wraps around the sphere, v clamped at the poles
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR);
        let sampler = unsafe { device.create_sampler(&sampler_info, None)? };

        // Post-pass sampler: clamp everywhere, so bloom near an edge never reads the opposite edge
        let post_sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR);
        let post_sampler = unsafe { device.create_sampler(&post_sampler_info, None)? };

        // Descriptor pool: 2 sets (planets, post), each with 2 image + sampler slots and 1 uniform buffer
        let pool_size = vk::DescriptorPoolSize::builder()
            .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(4);
        let uniform_pool_size = vk::DescriptorPoolSize::builder()
            .type_(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(2);
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::builder()
                    .pool_sizes(&[pool_size, uniform_pool_size])
                    .max_sets(2),
                None,
            )?
        };

        // Descriptor set: binding 0 → texture view + sampler
        let descriptor_set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::builder()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[descriptor_set_layout]),
            )?
        }[0];
        write_image_descriptor(&device, descriptor_set, 0, sampler, texture_view);
        write_image_descriptor(&device, descriptor_set, 2, sampler, sky_view);

        // Binding 1 of the planets' set → the bodies buffer
        let buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(bodies_buffer)
            .offset(0)
            .range(size_of::<BodiesUniform>() as u64);
        let buffer_infos = &[buffer_info];
        let bodies_write = vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(buffer_infos);
        unsafe { device.update_descriptor_sets(&[bodies_write], &[] as &[vk::CopyDescriptorSet]) };

        // Post descriptor set: binding 0 → scene image (same layout as the texture set)
        let post_descriptor_set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::builder()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[descriptor_set_layout]),
            )?
        }[0];
        write_image_descriptor(&device, post_descriptor_set, 0, post_sampler, scene_view);

        // 8. A unit sphere (radius 1), uploaded to the GPU. Drawn once per body, scaled to its radius
        let (sphere_vertices, sphere_indices) = uv_sphere(128, 64, 1.0, [0.0, 0.0, 0.0]);
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
        for module in [
            vert_module,
            frag_module,
            ray_vert_module,
            ray_frag_module,
            post_frag_module,
        ] {
            unsafe {
                device.destroy_shader_module(module, None);
            }
        }

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
            ray_pipeline_layout,
            ray_pipeline,
            vertex_buffer,
            vertex_memory,
            index_buffer,
            index_memory,
            index_count,
            depth_image,
            depth_memory,
            depth_view,
            scene_image,
            scene_memory,
            scene_view,
            post_pipeline_layout,
            post_pipeline,
            post_descriptor_set,
            texture_image,
            texture_memory,
            texture_view,
            sky_image,
            sky_memory,
            sky_view,
            sampler,
            post_sampler,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            bodies_buffer,
            bodies_memory,
        })
    }
}

impl Renderer {
    /// Draws one frame. Each body is drawn with its own transform matrix.
    ///
    /// Wait for the previous frame → get a swapchain image → record commands → submit → present.
    pub fn draw_frame(
        &mut self,
        pushes: &[PushConstants],
        ray_push: &RayPushConstants,
        retro: bool,
        bodies: &BodiesUniform,
    ) -> Result<()> {
        unsafe {
            // 1. Wait until the GPU has finished the previous frame, then re-arm the fence
            self.device
                .wait_for_fences(&[self.in_flight], true, u64::MAX)?;
            self.device.reset_fences(&[self.in_flight])?;

            // Copy this frame's bodies into the uniform buffer
            let ptr = self.device.map_memory(
                self.bodies_memory,
                0,
                size_of::<BodiesUniform>() as u64,
                vk::MemoryMapFlags::empty(),
            )?;
            std::ptr::copy_nonoverlapping(bodies, ptr.cast(), 1);
            self.device.unmap_memory(self.bodies_memory);

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
            self.record_commands(index, pushes, ray_push, retro)?;

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
    fn record_commands(
        &self,
        i: usize,
        pushes: &[PushConstants],
        ray_push: &RayPushConstants,
        retro: bool,
    ) -> Result<()> {
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

            // Barrier: switch the swapchain image and the scene image to "ready to draw colours".
            // UNDEFINED = we don't care about their old content, they get overwritten anyway
            let to_draw = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.swapchain.images[i])
                .subresource_range(range);
            let scene_to_draw = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.scene_image)
                .subresource_range(range);

            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, // wait for this stage...
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, // ...before this stage
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[to_draw, scene_to_draw],
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

            // Colour attachment: the scene image. Clear to black, keep the result (the post pass reads it)
            let clear = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            };

            let attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(self.scene_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear);
            let attachments = &[attachment];

            // Depth attachment: clear to 1.0 (= farthest), so anything drawn is closer.
            // DONT_CARE: the depths are only needed during the frame, not after
            let depth_clear = vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
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
            // Texture descriptor set (set 0)
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
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
            // Ray-tracing pass: one full-screen triangle (3 vertices, no buffer)
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.ray_pipeline,
            );
            self.device.cmd_push_constants(
                self.command_buffer,
                self.ray_pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                bytemuck::bytes_of(ray_push),
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
            // Back to the mesh pipeline for the planets
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
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

            // Barrier: the scene image goes from "being drawn" to "being read by a shader"
            let scene_to_read = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.scene_image)
                .subresource_range(range);
            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT, // wait for the scene to be written...
                vk::PipelineStageFlags::FRAGMENT_SHADER, // ...before the post shader reads it
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[scene_to_read],
            );

            // Post pass: draws into the swapchain image (every pixel is overwritten: no clear, no depth)
            let post_attachment = vk::RenderingAttachmentInfo::builder()
                .image_view(self.swapchain.image_views[i])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE);
            let post_attachments = &[post_attachment];
            let post_rendering = vk::RenderingInfo::builder()
                .render_area(vk::Rect2D::builder().extent(self.swapchain.extent))
                .layer_count(1)
                .color_attachments(post_attachments);

            self.device
                .cmd_begin_rendering(self.command_buffer, &post_rendering);
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.post_pipeline,
            );
            // Set 0 = the scene image
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.post_pipeline_layout,
                0,
                &[self.post_descriptor_set],
                &[],
            );
            let post_push = PostPushConstants {
                settings: [if retro { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            };
            self.device.cmd_push_constants(
                self.command_buffer,
                self.post_pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                bytemuck::bytes_of(&post_push),
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
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

    /// Destroys everything that depends on the window size: the swapchain, its image views,
    /// the depth image and the scene image. Used by `Drop`, and when the window is resized.
    fn destroy_swapchain(&mut self) {
        unsafe {
            // Views first, then the images they look at, then the memory behind them
            for &img_view in &self.swapchain.image_views {
                self.device.destroy_image_view(img_view, None);
            }
            self.device.destroy_image_view(self.depth_view, None);
            self.device.destroy_image(self.depth_image, None);
            self.device.free_memory(self.depth_memory, None);
            self.device.destroy_image_view(self.scene_view, None);
            self.device.destroy_image(self.scene_image, None);
            self.device.free_memory(self.scene_memory, None);
            // The swapchain last: it owns its images
            self.device
                .destroy_swapchain_khr(self.swapchain.handle, None);
        };
    }

    /// Rebuilds the swapchain, the depth image and the scene image at the window's new size.
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
            (self.depth_image, self.depth_memory, self.depth_view) = create_image(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.extent,
                DEPTH_FORMAT,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                1,
            )?;

            // New scene image, and point the post pass at it
            (self.scene_image, self.scene_memory, self.scene_view) = create_image(
                &self.instance,
                &self.device,
                self.physical_device,
                self.swapchain.extent,
                HDR_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::COLOR,
                1,
            )?;
            write_image_descriptor(
                &self.device,
                self.post_descriptor_set,
                0,
                self.post_sampler,
                self.scene_view,
            );
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
            self.device.destroy_buffer(self.bodies_buffer, None);
            self.device.free_memory(self.bodies_memory, None);
            // Texture: view, image, memory
            self.device.destroy_image_view(self.texture_view, None);
            self.device.destroy_image(self.texture_image, None);
            self.device.free_memory(self.texture_memory, None);
            self.device.destroy_image_view(self.sky_view, None);
            self.device.destroy_image(self.sky_image, None);
            self.device.free_memory(self.sky_memory, None);
            // Samplers + descriptors (the pool frees its sets)
            self.device.destroy_sampler(self.post_sampler, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            // Pipeline
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline(self.ray_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.ray_pipeline_layout, None);
            self.device.destroy_pipeline(self.post_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.post_pipeline_layout, None);
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
