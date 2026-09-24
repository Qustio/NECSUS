use crate::modules::renderer::gbuffers::GBuffers;
use crate::modules::renderer::pass::{self, AttachmentType, RenderTarget};
use crate::modules::renderer::vulkan_context::RenderFeature;
use crate::prelude::*;
use std::mem::swap;
use std::{cell::RefCell, error::Error, sync::Arc};

use crate::modules::core::EventQueue;
use crate::modules::window::Window;
use ash::vk::{self, Event};
use dear_imgui_ash::DynamicRendering;
use shipyard::{Unique, UniqueView};
use winit::event::WindowEvent;

use super::command_context::FrameCommand;
use super::frame_sync::FrameSync;
use super::pass::Pass;
use super::swapchain::Swapchain;
use super::vulkan_context::{Device, Instance};

pub trait UiDrawable: Send + Sync {
	fn draw(&self, ui: &dear_imgui_rs::Ui);
}

impl<F: Fn(&dear_imgui_rs::Ui) + Send + Sync> UiDrawable for F {
	fn draw(&self, ui: &dear_imgui_rs::Ui) {
		self(ui)
	}
}


#[derive(DDebug, Unique)]
pub struct ImguiState {
	pub context: RefCell<dear_imgui_rs::Context>,
	#[debug(skip)]
	pub renderer: RefCell<dear_imgui_ash::AshRenderer>,
	#[debug(skip)]
	pub platform: RefCell<dear_imgui_winit::WinitPlatform>,
	commands: Vec<FrameCommand>,
	image_format: vk::Format,
	window: Arc<winit::window::Window>,
	#[debug(skip)]
	render_target: RenderTarget,
	device: Arc<Device>
}

impl ImguiState {
	#[tracing::instrument(name = "ImguiState::new", skip_all)]
	pub(super) fn new(
		instance: Arc<Instance>,
		device: Arc<Device>,
		swapchain: &Swapchain,
		window: Arc<winit::window::Window>,
		ini_path: Option<std::path::PathBuf>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let frame_count = swapchain.frame_count as usize;
		let commands = (0..frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
			.collect::<Result<Vec<_>, _>>()?;

		let mut context = dear_imgui_rs::Context::create();
		// default ini_filename ("imgui.ini") resolves relative to cwd, which doesn't
		// exist as writable storage on Android - point it at the app's private data dir.
		if ini_path.is_some() {
			context.set_ini_filename(ini_path)?;
		}

		let mut platform = dear_imgui_winit::WinitPlatform::new(&mut context)?;

		#[cfg(not(target_os = "android"))]
		let dpi = dear_imgui_winit::HiDpiMode::Rounded;
		#[cfg(target_os = "android")]
		let dpi = dear_imgui_winit::HiDpiMode::Default;

		platform.attach_window(
			window.clone(),
			dpi,
			&mut context,
		)?;
		platform.set_ime_allowed(false)?;
		platform.set_ime_auto_management(true);

		let queue = *device.graphics_queue.lock().expect("couldnt lock queue");

		let render_target = RenderTarget::new(
			&device,
			&swapchain,
			&[
				(AttachmentType::Image, swapchain.format.format, &swapchain.image_views,  vk::AttachmentLoadOp::LOAD, vk::AttachmentStoreOp::STORE),
			],
		)?;
		let renderer_confgi = match (&device.render_feature, &render_target) {
			(RenderFeature::RenderPass, RenderTarget::RenderPass{render_pass, ..}) => {
				dear_imgui_ash::AshRendererConfig::with_render_pass(
					device.device.clone(),
					queue,
					commands[0].pool,
					*render_pass
				)
			},
			_ => {
				dear_imgui_ash::AshRendererConfig::with_dynamic_rendering(
					device.device.clone(),
					queue,
					commands[0].pool,
					DynamicRendering {
						color_attachment_format: swapchain.format.format,
						depth_attachment_format: None,
					}
				)
			},
		}.with_options(dear_imgui_ash::Options {
			in_flight_frames: frame_count as usize,
			framebuffer_srgb: true,
			..Default::default()
		});
		let renderer = unsafe {
			dear_imgui_ash::AshRenderer::with_default_allocator(&instance, device.physical_device, renderer_confgi, &mut context)
		}?;

		Ok(Self {
			context: RefCell::new(context),
			renderer: RefCell::new(renderer),
			platform: RefCell::new(platform),
			commands,
			image_format: swapchain.format.format,
			window,
			render_target,
			device
		})
	}

	pub(super) fn reattach_window(
		&mut self,
		window: Arc<winit::window::Window>
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let mut context = self.context.borrow_mut();
		let mut platform = self.platform.borrow_mut();
		platform.detach_window(&mut context)?;
		#[cfg(not(target_os = "android"))]
		let dpi = dear_imgui_winit::HiDpiMode::Rounded;
		#[cfg(target_os = "android")]
		let dpi = dear_imgui_winit::HiDpiMode::Default;

		platform.attach_window(
			window.clone(),
			dpi,
			&mut context,
		)?;
		platform.set_ime_allowed(false)?;
		platform.set_ime_auto_management(true);
		self.window = window;
		Ok(())
	}

	pub(super) fn handle_events(
		&mut self,
		events: UniqueView<EventQueue<WindowEvent>>,
		window: UniqueView<Window>,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let mut ctx = self.context.borrow_mut();
		for event in &events.events {
			self.platform
				.borrow_mut()
				.handle_window_event(&mut ctx, &window.window, event)?;
		}
		self.platform
			.borrow_mut()
			.prepare_frame(&mut ctx, &window.window)?;
		Ok(())
	}

	pub(super) fn record(
		&self,
		record_view: &pass::RecordView,
		// window: &winit::window::Window,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let frame_sync = &record_view.frame_sync;
		let swapchain = &record_view.swapchain;
		let draws = &record_view.imgui_draws;

		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		unsafe {
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;
			match &self.render_target {
				RenderTarget::RenderPass { render_pass, .. } => {
					cmd.device.begin_command_buffer(
						cmd.buffer,
						&vk::CommandBufferBeginInfo::default()
							.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE)
							.inheritance_info(
								&vk::CommandBufferInheritanceInfo::default()
									.render_pass(*render_pass)
									.subpass(0)
							),
					)?;
				},
				RenderTarget::DynamicRendering => {
					cmd.device.begin_command_buffer(
						cmd.buffer,
						&vk::CommandBufferBeginInfo::default()
							.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
							.inheritance_info(
								&vk::CommandBufferInheritanceInfo::default().push_next(
									&mut vk::CommandBufferInheritanceRenderingInfo::default()
										.color_attachment_formats(&[self.image_format])
										.rasterization_samples(vk::SampleCountFlags::TYPE_1),
								),
							),
					)?;
					if let RenderFeature::DynamicRendering(device) = &self.device.render_feature {
						device.cmd_begin_rendering(
							cmd.buffer,
							&vk::RenderingInfo::default()
								.render_area(vk::Rect2D::default().extent(swapchain.extent))
								.layer_count(1)
								.color_attachments(&[vk::RenderingAttachmentInfo::default()
									.image_view(image_view)
									.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
									.load_op(vk::AttachmentLoadOp::LOAD)
									.store_op(vk::AttachmentStoreOp::STORE)]),
						);
					}
				},
			}
		}
		let mut ctx = self.context.borrow_mut();
		let ui = ctx.frame();
		for item in draws.iter() {
			item.draw(ui);
		}
		// todo get it out
		self.platform.borrow_mut().prepare_render(ui, &self.window)?;
		let pending = ctx.render(self.renderer.borrow().renderer_consumer()?);
		let (draw_data, _) = self.renderer.borrow_mut().prepare_frame(pending)?;
		unsafe {
			self.renderer
				.borrow_mut()
				.cmd_draw(cmd.buffer, draw_data)
				.unwrap();
			match &self.render_target {
				RenderTarget::RenderPass { .. } => {
					
				},
				RenderTarget::DynamicRendering => {
					if let RenderFeature::DynamicRendering(device) = &self.device.render_feature {
						device.cmd_end_rendering(cmd.buffer);
					}
				},
			}
			cmd.device.end_command_buffer(cmd.buffer)?;
		}
		Ok(())
	}
}

impl Pass for ImguiState {
	fn buffers(&self, frame_sync: &FrameSync) -> Vec<vk::CommandBuffer> {
		vec![self.commands[frame_sync.frame_id as usize].buffer]
	}
	fn render_target(&self) -> &RenderTarget {
		&self.render_target
	}
	
	fn record(&self, record_view: &pass::RecordView) {
		_ = self.record(record_view);
	}
	
	fn resize(&mut self, swapchain: &Swapchain, _: &GBuffers) -> Result<(), Box<dyn Error + Send + Sync>> {
		self.render_target.resize(&self.device, swapchain, &[
			(AttachmentType::Image, swapchain.format.format, &swapchain.image_views,  vk::AttachmentLoadOp::LOAD, vk::AttachmentStoreOp::STORE),
		])
	}
}

impl Drop for ImguiState {
	fn drop(&mut self) {
		let _ = self
			.renderer
			.borrow_mut()
			.shutdown(&mut self.context.borrow_mut());
	}
}

unsafe impl Send for ImguiState {}
unsafe impl Sync for ImguiState {}
