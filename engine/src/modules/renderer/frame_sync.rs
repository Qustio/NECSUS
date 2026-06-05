use std::{default::Default, error::Error, sync::Arc};

use super::vulkan_context::Device;
use ash::{prelude::VkResult, *};
use itertools::Itertools;
use shipyard::Unique;

#[derive(Unique)]
#[allow(dead_code)]
pub struct FrameSync {
	pub(super) image_availabe: Vec<vk::Semaphore>,
	pub(super) render_finished: Vec<vk::Semaphore>,
	pub(super) fences: Vec<vk::Fence>,
	pub frame_count: u32,
	pub frame_id: u32,
	pub acquired_image_index: u32,
	device: Arc<Device>,
}

impl FrameSync {
	pub(super) fn new(
		device: Arc<Device>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let (image_availabe, render_finished, fences): (Vec<_>, Vec<_>, Vec<_>) = (0..frame_count)
			.map(|_| unsafe {
				Ok::<_, vk::Result>((
					device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
					device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
					device.create_fence(
						&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
						None,
					)?,
				))
			})
			.collect::<VkResult<Vec<_>>>()?
			.into_iter()
			.multiunzip();
		Ok(Self {
			image_availabe,
			render_finished,
			fences,
			frame_count,
			frame_id: 0,
			acquired_image_index: 0,
			device,
		})
	}

	pub(super) fn wait(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _span = tracy_client::span!();
		unsafe {
			self.device
				.wait_for_fences(&[self.fences[self.frame_id as usize]], true, u64::MAX)?;
			self.device
				.reset_fences(&[self.fences[self.frame_id as usize]])?;
		};
		Ok(())
	}
}

impl Drop for FrameSync {
	fn drop(&mut self) {
		unsafe {
			let _ = self.device.device_wait_idle();
			for &s in &self.image_availabe {
				self.device.destroy_semaphore(s, None);
			}
			for &s in &self.render_finished {
				self.device.destroy_semaphore(s, None);
			}
			for &f in &self.fences {
				self.device.destroy_fence(f, None);
			}
		}
	}
}
