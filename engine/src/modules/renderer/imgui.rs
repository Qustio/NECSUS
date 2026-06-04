use std::{cell::RefCell, error::Error, sync::Arc};

use ash::vk;
use shipyard::{Unique, UniqueView};
use winit::event::WindowEvent;
use crate::modules::core::EventQueue;
use crate::modules::window::Window;

use super::command_context::FrameCommand;
use super::frame_sync::FrameSync;
use super::pass::Pass;
use super::swapchain::Swapchain;
use super::vulkan_context::{Device, Instance};

pub trait UiDrawable: Send + Sync {
    fn draw(&self, ui: &imgui::Ui);
}

impl<F: Fn(&imgui::Ui) + Send + Sync> UiDrawable for F {
	fn draw(&self, ui: &imgui::Ui) {
		self(ui)
	}
}

#[derive(Unique, Default)]
pub struct UiDrawList {
    pub items: Vec<Box<dyn UiDrawable + Send + Sync>>,
}

#[derive(Unique)]
pub struct ImguiState {
    pub context: RefCell<imgui::Context>,
    pub renderer: RefCell<imgui_rs_vulkan_renderer::Renderer>,
    pub platform: imgui_winit_support::WinitPlatform,
    commands: Vec<FrameCommand>,
    image_format: vk::Format,
}

impl ImguiState {
    pub(super) fn new(
        instance: Arc<Instance>,
        device: Arc<Device>,
        frame_count: u32,
        color_format: vk::Format,
        window: &winit::window::Window,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let commands = (0..frame_count)
            .map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
            .collect::<Result<Vec<_>, _>>()?;

        let mut context = imgui::Context::create();

        let mut platform = imgui_winit_support::WinitPlatform::new(&mut context);
        platform.attach_window(
            context.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Rounded,
        );

        let queue = *device.graphics_queue.lock().expect("couldnt lock queue");
        let renderer = imgui_rs_vulkan_renderer::Renderer::with_default_allocator(
            &instance.instance,
            device.physical_device,
            device.device.clone(),
            queue,
            commands[0].pool,
            imgui_rs_vulkan_renderer::DynamicRendering {
                color_attachment_format: color_format,
                depth_attachment_format: None,
            },
            &mut context,
            None,
        )?;

        Ok(Self {
            context: RefCell::new(context),
            renderer: RefCell::new(renderer),
            platform,
            commands,
            image_format: color_format,
        })
    }

	pub(super) fn handle_events(
		&mut self,
		events: UniqueView<EventQueue<WindowEvent>>,
		window: UniqueView<Window>
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let mut ctx = self.context.borrow_mut();
		for event in &events.events {
			self.platform.handle_window_event(ctx.io_mut(), &window.window, event);
		}
		self.platform.prepare_frame(ctx.io_mut(), &window.window)?;
		Ok(())
	}

    pub(super) fn record(&self, frame_sync: &FrameSync, swapchain: &Swapchain, draws: &mut UiDrawList) {
        let id = frame_sync.frame_id as usize;
        let id_image = frame_sync.acquired_image_index as usize;
        let cmd = &self.commands[id];
        let image_view = swapchain.image_views[id_image];
        unsafe {
            cmd.device.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty());
            cmd.device.begin_command_buffer(
                cmd.buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                    .inheritance_info(&vk::CommandBufferInheritanceInfo::default()
                    .push_next(&mut vk::CommandBufferInheritanceRenderingInfo::default()
                        .color_attachment_formats(&[self.image_format])
                        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                    ))
            ).unwrap();
            cmd.device.cmd_begin_rendering(
                cmd.buffer,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D::default().extent(swapchain.extent))
                    .layer_count(1)
                    .color_attachments(&[vk::RenderingAttachmentInfo::default()
                        .image_view(image_view)
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::LOAD)
                        .store_op(vk::AttachmentStoreOp::STORE)
                    ]),
            );
        }
        let mut ctx = self.context.borrow_mut();
        let ui = ctx.new_frame();
        for item in draws.items.drain(..) {
            item.draw(ui);
        }
        let draw_data = ctx.render();
        self.renderer.borrow_mut().cmd_draw(cmd.buffer, draw_data).unwrap();
        unsafe {
            cmd.device.cmd_end_rendering(cmd.buffer);
            cmd.device.end_command_buffer(cmd.buffer).unwrap();
        }
    }
}

impl Pass for ImguiState {
    fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
        self.commands[frame_sync.frame_id as usize].buffer
    }
}

unsafe impl Send for ImguiState {}
unsafe impl Sync for ImguiState {}
