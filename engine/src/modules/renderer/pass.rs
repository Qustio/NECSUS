use std::ops::Deref;
use std::{error::Error, sync::Arc};
use ash::*;
use shipyard::{Unique, UniqueView};

use super::frame_sync::FrameSync;
use super::vulkan_context::Device;
use super::command_context::FrameCommand;

pub(crate) trait Pass {
	fn record(&self, frame_sync: &FrameSync);
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer;
}

impl<T: Pass + Unique> Pass for UniqueView<'_, T> {
	fn record(&self, frame_sync: &FrameSync) {
		self.deref().record(frame_sync);
	}

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
}

impl Pass for Main {
	fn record(&self, frame_sync: &FrameSync) {
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

	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}

#[derive(Unique)]
pub struct Back {
	commands: Vec<FrameCommand>,
	image_format: vk::Format,
}

impl Back {
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
}

impl Pass for Back {
	fn record(&self, frame_sync: &FrameSync) {
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

	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}
