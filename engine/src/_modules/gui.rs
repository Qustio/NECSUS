use itertools::Itertools;
use vulkano::{VulkanObject, ash::Entry, command_buffer::AutoCommandBufferBuilder};

use std::{cell::{Cell, RefCell, UnsafeCell}, collections::HashMap, error::Error, ops::DerefMut, sync::{Arc, Mutex}};

use shipyard::*;
use crate::modules::*;

use super::{pipeline, RenderTarget, Renderable};

use derive_where::derive_where;


#[derive_where(Debug)]
#[derive(Unique)]
pub struct ImguiRenderable {
    #[derive_where(skip)]
    device: Device,
    #[derive_where(skip)]
    pub imgui_renderer: imgui_rs_vulkan_renderer::Renderer,
    pub imgui_context: imgui::Context,
    pub imgui_platform: imgui_winit_support::WinitPlatform,
    pub ui: Option<*mut imgui::Ui>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Component)]
pub struct BM;

impl ImguiRenderable {
    pub fn new(
        vulkan_context: &vulkan_context::VulkanContext,
        swapchain: &swapchain::Swapchain,
        window: &window::Window
    ) -> Result<Self, Box<dyn Error>> {
        let device = vulkan_context.device.clone();

        let mut imgui_context = imgui::Context::create();

        let mut imgui_platform = imgui_winit_support::WinitPlatform::new(&mut imgui_context);
        imgui_platform.attach_window(
            imgui_context.io_mut(),
            &window.window,
            imgui_winit_support::HiDpiMode::Rounded,
        );

		let instance = unsafe { vulkano::ash::Entry::load()? };
		let e = unsafe { Entry::load()?};

		let ash_instance = unsafe {
            vulkano::ash::Instance::load(
                vulkan_context.instance.library().fp_v1_0(),
                vulkan_context.instance.handle(),
            )
        };
		let ddevice = unsafe {vulkano::ash::Device::load(vulkan_context.instance.library().fp_v1_0(), device.handle())};

		let command_pool = AutoCommandBufferBuilder::primary(
            vulkan_context.command_buffer_allocator,
            vulkan_context.graphics_queue.queue_family_index(),
            vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap()
        .build()
        .unwrap();
	
		

        let imgui_renderer = imgui_rs_vulkan_renderer::Renderer::with_default_allocator(
            &ash_instance,
            vulkan_context.physical_device.handle(),
            ddevice,
            vulkan_context.graphics_queue.handle(),
            command_pool,
            swapchain.render_pass,
            &mut imgui_context,
            Some(imgui_rs_vulkan_renderer::Options {
                in_flight_frames: swapchain.frames_count as usize,
                ..Default::default()
            }),
        )
        .unwrap();

        Ok(Self {
            device,
            imgui_renderer,
            imgui_context,
            imgui_platform,
            ui: None
        })
    }

    pub fn prepare_frame(
        &mut self, window: &winit::window::Window,
    ) {
        // let io = self.imgui_context.io_mut();
		// self.imgui_platform.prepare_frame(io, window).unwrap();
        // for window_event in window_events.read() {
        //     self.imgui_platform.handle_window_event(io, window, &window_event.window);
        // }
        // let ui = self.imgui_context.new_frame();
        // self.ui = Some(ui as *mut _);
    }

    pub fn d(
        &mut self,
        command_buffer: vk::CommandBuffer,
    ) {
        let draw_data = self.imgui_context.render();
        self.imgui_renderer.cmd_draw(command_buffer, draw_data).unwrap();
    }
}

impl Renderable for ImguiRenderable {
    fn draw(
        &mut self,
        command_buffer: vk::CommandBuffer,
        material_pass: RenderTarget,
        world: &mut World,
        frame: u32,
    ) -> Result<(), Box<dyn Error>> {
        // match material_pass {
        //     RenderTarget::DebugPass => {
        //         let draw_data = self.imgui_context.render();
        //         self.imgui_renderer.cmd_draw(command_buffer, draw_data)?;
				
        //     }
        //     _ => (),
        // }
        Ok(())
    }

    fn update(&mut self, world: &mut World) -> Result<(), Box<dyn Error>> {
		// let mut to_despawn = Vec::new();
		// let window = world.get_resource::<Window>().unwrap();
		// let window_ref = &window.window;
		// let io = self.imgui_context.io_mut();
		// self.imgui_platform.prepare_frame(io, window_ref)?;

		// // Handle events
        // {
        //     let mut events = world.try_query::<(Entity, &ImguiEvent)>().unwrap();
        //     for (e, event) in events.iter(world) {
				
        //         self.imgui_platform
        //             .handle_window_event(io, window_ref, &event.0);
		// 		to_despawn.push(e);
        //     }
        // }

        // // Handle draw callbacks
		// {
		// 	let ui = self.imgui_context.new_frame();
		// 	let mut callbacks = world.query::<(Entity, &mut ImguiContext)>();

		// 	for (e, mut callback) in callbacks.iter_mut(world) {
				
		// 		callback.0(ui)?;
		// 		to_despawn.push(e);
		// 	}

		// 	ui.end_frame_early();
		// }

		// {
		// 	for e in to_despawn {
        //     	world.entity_mut(e).despawn();
		// 	}
		// }
        Ok(())
    }
}

pub type ImguiCallback = Box<dyn FnMut(&mut imgui::Ui) -> Result<(), Box<dyn Error>> + Send + Sync>;

#[derive(Component)]
pub struct ImguiContext(ImguiCallback);

impl ImguiContext {
    pub fn new(imgui_callback: ImguiCallback) -> Self {
        Self(imgui_callback)
    }
}