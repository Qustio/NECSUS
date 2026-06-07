use super::gbuffers::GBuffers;
use super::vulkan_context::Device;
use super::{debug_tools::FrameCapture, frame_sync::FrameSync, pass::Pass, swapchain::Swapchain};

use ash::*;
use shipyard::Unique;
use std::sync::atomic::Ordering;
use std::{error::Error, sync::Arc};

pub struct FrameCommand {
	pub(super) buffer: vk::CommandBuffer,
	pub(super) pool: vk::CommandPool,
	pub(super) device: Arc<Device>,
}

impl FrameCommand {
	pub(super) fn new(
		device: Arc<Device>,
		level: vk::CommandBufferLevel,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let pool = unsafe {
			device.create_command_pool(
				&vk::CommandPoolCreateInfo::default()
					.queue_family_index(device.graphics_queue_index)
					.flags(vk::CommandPoolCreateFlags::TRANSIENT),
				None,
			)?
		};
		let buffer = unsafe {
			device.allocate_command_buffers(
				&vk::CommandBufferAllocateInfo::default()
					.command_buffer_count(1)
					.command_pool(pool)
					.level(level),
			)?[0]
		};
		Ok(Self {
			buffer,
			pool,
			device,
		})
	}
}

impl Drop for FrameCommand {
	fn drop(&mut self) {
		unsafe {
			let _ = self.device.wait_queue();
			self.device.destroy_command_pool(self.pool, None);
		}
	}
}

#[derive(Unique)]
pub struct CommandContext {
	pub commands: Vec<FrameCommand>,
	pub capture_frame: bool,
}

impl CommandContext {
	pub(super) fn new(
		device: Arc<Device>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let frames = (0..frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::PRIMARY))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			commands: frames,
			capture_frame: false,
		})
	}

	pub(super) fn begin(&self, id: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let cmd = &self.commands[id as usize];
		unsafe {
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;
			cmd.device.begin_command_buffer(
				cmd.buffer,
				&vk::CommandBufferBeginInfo::default()
					.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
			)?;
		}
		Ok(())
	}

	pub(super) fn to_optimal(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image = swapchain.images[id_image];
		let depth = gbuffers[id_image].depth.image;

		unsafe {
			cmd.device.cmd_pipeline_barrier2(
				cmd.buffer,
				&vk::DependencyInfo::default().image_memory_barriers(&[
					vk::ImageMemoryBarrier2::default()
						.src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
						.dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
						.src_access_mask(vk::AccessFlags2::NONE)
						.dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
						.old_layout(vk::ImageLayout::UNDEFINED)
						.new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
						.image(image)
						.subresource_range(vk::ImageSubresourceRange {
							aspect_mask: vk::ImageAspectFlags::COLOR,
							level_count: 1,
							layer_count: 1,
							..Default::default()
						}),
				]),
			);
			cmd.device.cmd_pipeline_barrier2(
				cmd.buffer,
				&vk::DependencyInfo::default().image_memory_barriers(&[
					vk::ImageMemoryBarrier2::default()
						.src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
						.dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
						.src_access_mask(vk::AccessFlags2::NONE)
						.dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
						.old_layout(vk::ImageLayout::UNDEFINED)
						.new_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
						.image(depth)
						.subresource_range(vk::ImageSubresourceRange {
							aspect_mask: vk::ImageAspectFlags::DEPTH,
							level_count: 1,
							layer_count: 1,
							..Default::default()
						}),
				]),
			);
		}
		Ok(())
	}

	pub(super) fn swapchain_to_present(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
		capture: &FrameCapture,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image = swapchain.images[id_image];

		let color_range = vk::ImageSubresourceRange {
			aspect_mask: vk::ImageAspectFlags::COLOR,
			level_count: 1,
			layer_count: 1,
			..Default::default()
		};

		unsafe {
			if !capture.pending.load(Ordering::Relaxed) {
				cmd.device.cmd_pipeline_barrier2(
					cmd.buffer,
					&vk::DependencyInfo::default().image_memory_barriers(&[
						vk::ImageMemoryBarrier2::default()
							.src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
							.dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
							.src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
							.dst_access_mask(vk::AccessFlags2::NONE)
							.old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
							.new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
							.image(image)
							.subresource_range(color_range),
					]),
				);
			} else {
				// swapchain: COLOR_ATTACHMENT_OPTIMAL -> TRANSFER_SRC_OPTIMAL
				cmd.device.cmd_pipeline_barrier2(
					cmd.buffer,
					&vk::DependencyInfo::default().image_memory_barriers(&[
						vk::ImageMemoryBarrier2::default()
							.src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
							.dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
							.src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
							.dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
							.old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
							.new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
							.image(image)
							.subresource_range(color_range),
					]),
				);
				// capture image: UNDEFINED -> TRANSFER_DST_OPTIMAL
				cmd.device.cmd_pipeline_barrier2(
					cmd.buffer,
					&vk::DependencyInfo::default().image_memory_barriers(&[
						vk::ImageMemoryBarrier2::default()
							.src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
							.dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
							.src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
							.dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
							.old_layout(vk::ImageLayout::UNDEFINED)
							.new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
							.image(capture.image.0)
							.subresource_range(color_range),
					]),
				);
				let blit_regions = &[vk::ImageBlit2::default()
					.src_subresource(
						vk::ImageSubresourceLayers::default()
							.aspect_mask(vk::ImageAspectFlags::COLOR)
							.mip_level(0)
							.base_array_layer(0)
							.layer_count(1),
					)
					.src_offsets([
						vk::Offset3D { x: 0, y: 0, z: 0 },
						vk::Offset3D {
							x: swapchain.extent.width as i32,
							y: swapchain.extent.height as i32,
							z: 1,
						},
					])
					.dst_subresource(
						vk::ImageSubresourceLayers::default()
							.aspect_mask(vk::ImageAspectFlags::COLOR)
							.mip_level(0)
							.base_array_layer(0)
							.layer_count(1),
					)
					.dst_offsets([
						vk::Offset3D { x: 0, y: 0, z: 0 },
						vk::Offset3D {
							x: capture.extent.width as i32,
							y: capture.extent.height as i32,
							z: 1,
						},
					])];
				let blit_info = &vk::BlitImageInfo2::default()
					.src_image(image)
					.dst_image(capture.image.0)
					.src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
					.dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
					.filter(vk::Filter::LINEAR)
					.regions(blit_regions);

				cmd.device.cmd_blit_image2(cmd.buffer, blit_info);
				// capture image: TRANSFER_DST_OPTIMAL -> TRANSFER_SRC_OPTIMAL
				cmd.device.cmd_pipeline_barrier2(
					cmd.buffer,
					&vk::DependencyInfo::default().image_memory_barriers(&[
						vk::ImageMemoryBarrier2::default()
							.src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
							.dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
							.src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
							.dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
							.old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
							.new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
							.image(capture.image.0)
							.subresource_range(color_range),
					]),
				);
				let region = vk::BufferImageCopy {
					buffer_offset: 0,
					buffer_row_length: 0,   // 0 = tightly packed (rows = width)
					buffer_image_height: 0, // 0 = tightly packed (height)
					image_subresource: vk::ImageSubresourceLayers {
						aspect_mask: vk::ImageAspectFlags::COLOR,
						mip_level: 0,
						base_array_layer: 0,
						layer_count: 1,
					},
					image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
					image_extent: vk::Extent3D {
						width: capture.extent.width,
						height: capture.extent.height,
						depth: 1,
					},
				};
				// copy capture image
				cmd.device.cmd_copy_image_to_buffer(
					cmd.buffer,
					capture.image.0,
					vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
					capture.buffer.buffer(),
					&[region],
				);
				// swapchain: TRANSFER_SRC_OPTIMAL -> PRESENT_SRC_KHR
				cmd.device.cmd_pipeline_barrier2(
					cmd.buffer,
					&vk::DependencyInfo::default().image_memory_barriers(&[
						vk::ImageMemoryBarrier2::default()
							.src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
							.dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
							.src_access_mask(vk::AccessFlags2::TRANSFER_READ)
							.dst_access_mask(vk::AccessFlags2::NONE)
							.old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
							.new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
							.image(image)
							.subresource_range(color_range),
					]),
				);
			}
		}
		Ok(())
	}

	pub(super) fn begin_rendering(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		let depth_view = gbuffers[id_image].depth.view;
		unsafe {
			cmd.device.cmd_begin_rendering(
				cmd.buffer,
				&vk::RenderingInfo::default()
					.render_area(vk::Rect2D::default().extent(swapchain.extent))
					.layer_count(1)
					.color_attachments(&[
						vk::RenderingAttachmentInfo::default()
							.image_view(image_view)
							.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
							.load_op(vk::AttachmentLoadOp::CLEAR)
							.store_op(vk::AttachmentStoreOp::STORE)
							.clear_value(vk::ClearValue {
								color: vk::ClearColorValue { float32: [0.0; 4] },
							}),
					])
					.depth_attachment(&vk::RenderingAttachmentInfo::default()
						.image_view(depth_view)
						.image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
						.load_op(vk::AttachmentLoadOp::CLEAR)
						.store_op(vk::AttachmentStoreOp::STORE)
						.clear_value(vk::ClearValue {
							depth_stencil: vk::ClearDepthStencilValue {
								depth: 0.0,
								stencil: 0,
							},
					}))
			);
		}
		Ok(())
	}

	pub(super) fn execute_commands(&self, frame_sync: &FrameSync, cmds: &[&dyn Pass]) {
		let id = frame_sync.frame_id as usize;
		let primary_cmd = &self.commands[id];
		let buffers = cmds
			.iter()
			.map(|cmd| cmd.buffer(frame_sync))
			.collect::<Vec<_>>();
		unsafe {
			primary_cmd
				.device
				.cmd_execute_commands(primary_cmd.buffer, &buffers);
		}
	}

	pub(super) fn end_rendering(
		&self,
		frame_sync: &FrameSync,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let id = frame_sync.frame_id as usize;
		let cmd = &self.commands[id];
		unsafe {
			cmd.device.cmd_end_rendering(cmd.buffer);
		}
		Ok(())
	}

	pub(super) fn end(&self, frame_sync: &FrameSync) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let id = frame_sync.frame_id as usize;
		let cmd = &self.commands[id];
		unsafe {
			cmd.device.end_command_buffer(cmd.buffer)?;
		}
		Ok(())
	}

	pub(super) fn submit(
		&self,
		frame_sync: &FrameSync,
		capture: &FrameCapture,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		let frame = frame_sync.frame_id as usize;
		let cmd = &self.commands[frame];
		unsafe {
			let queue = cmd
				.device
				.graphics_queue
				.lock()
				.expect("couldnt lock queue");
			cmd.device.queue_submit2(
				*queue,
				&[vk::SubmitInfo2::default()
					.wait_semaphore_infos(&[vk::SemaphoreSubmitInfo::default()
						.semaphore(frame_sync.image_availabe[frame_sync.acquired_image_index as usize])
						.stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)])
					.command_buffer_infos(&[
						vk::CommandBufferSubmitInfo::default().command_buffer(cmd.buffer)
					])
					.signal_semaphore_infos(&[vk::SemaphoreSubmitInfo::default()
						.semaphore(frame_sync.render_finished[frame_sync.acquired_image_index as usize])
						.stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)])],
				frame_sync.fences[frame],
			)?;
			let exchange = capture
				.pending
				.compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
				.unwrap_or(false);
			if exchange {
				cmd.device
					.wait_for_fences(&[frame_sync.fences[frame]], true, u64::MAX)?;
				capture.copy()?;
			}
		};
		Ok(())
	}
}
