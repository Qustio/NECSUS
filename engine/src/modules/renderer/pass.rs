use std::ops::Deref;
use std::{error::Error, sync::Arc};
use ash::*;
use shipyard::{Unique, UniqueView};

use crate::modules::renderer::swapchain::Swapchain;

use super::frame_sync::FrameSync;
use super::vulkan_context::{Device, Allocator};
use super::pipeline::Pipeline;
use super::buffer::Buffer;
use super::command_context::FrameCommand;

pub(crate) trait Pass {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer;
}

impl<T: Pass + Unique> Pass for UniqueView<'_, T> {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		self.deref().buffer(frame_sync)
	}
}

#[derive(Unique)]
pub struct Main {
	commands: Vec<FrameCommand>,
	image_format: vk::Format,
}

impl Main {
	pub(super) fn new(
		device: Arc<Device>,
		frame_count: u32,
		image_format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let commands = (0..frame_count)
            .map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self{
			commands,
			image_format,
		})
    }

	pub(super) fn record(&self, frame_sync: &FrameSync, swapchain: &Swapchain) {
		let id = frame_sync.frame_id as usize;
		let cmd = &self.commands[id];
		unsafe {
			// reset all buffers in pool
			cmd.device.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty());
			cmd.device.begin_command_buffer(
				cmd.buffer,
				&vk::CommandBufferBeginInfo::default()
					.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
					.inheritance_info(&vk::CommandBufferInheritanceInfo::default()
					.push_next(&mut vk::CommandBufferInheritanceRenderingInfo::default()
						.color_attachment_formats(&[self.image_format])
						.rasterization_samples(vk::SampleCountFlags::TYPE_1)
					)
				)
			);
			// cmd beginredering
			// cmd draw
			// cmd endrendering
			cmd.device.end_command_buffer(cmd.buffer);
		}
	}
}

impl Pass for Main {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}

#[derive(Unique)]
pub struct Back {
	commands: Vec<FrameCommand>,
	pipeline: Pipeline,
	vertexes: Buffer,
	indexes: Buffer,
	image_format: vk::Format,
}

impl Back {
	pub(super) fn new(
		device: Arc<Device>,
		allocator: Arc<Allocator>,
		frame_count: u32,
		image_format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let commands = (0..frame_count)
            .map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
            .collect::<Result<Vec<_>, _>>()?;
		let pipeline = Pipeline::new(
			device,
			image_format
		)?;
		let vertexes = Buffer::from_slice(
			allocator.clone(),
			&[
				nalgebra_glm::Vec2::new(-1.0, -1.0),
				nalgebra_glm::Vec2::new(-1.0, 1.0),
				nalgebra_glm::Vec2::new(1.0, -1.0),
				nalgebra_glm::Vec2::new(1.0, 1.0),
			],
			vk::BufferUsageFlags::VERTEX_BUFFER
		)?;
		let indexes = Buffer::from_slice::<u32>(
			allocator.clone(),
			&[0, 1, 3, 0, 3, 2],
			vk::BufferUsageFlags::INDEX_BUFFER
		)?;
        Ok(Self{
			commands,
			pipeline,
			vertexes,
			indexes,
			image_format,
		})
    }

	pub(super) fn record(&self, frame_sync: &FrameSync, swapchain: &Swapchain) {
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		unsafe {
			// reset all buffers in pool
			cmd.device.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty());
			cmd.device.begin_command_buffer(
				cmd.buffer,
				&vk::CommandBufferBeginInfo::default()
					.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
					.inheritance_info(&vk::CommandBufferInheritanceInfo::default()
					.push_next(&mut vk::CommandBufferInheritanceRenderingInfo::default()
						.color_attachment_formats(&[self.image_format])
						.rasterization_samples(vk::SampleCountFlags::TYPE_1)
					)
				)
			);
			cmd.device.cmd_begin_rendering(cmd.buffer, &vk::RenderingInfo::default()
				.render_area(vk::Rect2D::default().extent(swapchain.extent))
					.layer_count(1)
					.color_attachments(&[vk::RenderingAttachmentInfo::default()
						.image_view(image_view)
						.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
						.load_op(vk::AttachmentLoadOp::LOAD)
						.store_op(vk::AttachmentStoreOp::STORE)
					]),
			);
			cmd.device.cmd_set_viewport(cmd.buffer, 0, &[vk::Viewport {
				x: 0.0, y: 0.0,
				width: swapchain.extent.width as f32,
				height: swapchain.extent.height as f32,
				min_depth: 0.0,
				max_depth: 1.0,
			}]);
			cmd.device.cmd_set_scissor(cmd.buffer, 0, &[vk::Rect2D {
				offset: vk::Offset2D { x: 0, y: 0 },
				extent: swapchain.extent,
			}]);
			cmd.device.cmd_bind_pipeline(cmd.buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline.pipeline());
			cmd.device.cmd_bind_vertex_buffers(cmd.buffer, 0, &[self.vertexes.buffer()], &[0]);
			cmd.device.cmd_bind_index_buffer(cmd.buffer, self.indexes.buffer(), 0, vk::IndexType::UINT32);
			cmd.device.cmd_draw_indexed(cmd.buffer, 6, 1, 0, 0, 0);
			// cmd draw
			cmd.device.cmd_end_rendering(cmd.buffer);
			cmd.device.end_command_buffer(cmd.buffer);
		}
	}
}

impl Pass for Back {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}
