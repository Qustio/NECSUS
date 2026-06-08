use std::error::Error;
use std::sync::Arc;

use ash::*;
use shipyard::Unique;

use super::vulkan_context::{Allocator, Device};
use super::allocated_image::AllocatedImage;

#[derive(Unique, derive_more::Deref)]
pub(super) struct GBuffers {
	#[deref]
	gbuffers: Vec<GBuffer>,
}

pub(super) struct GBuffer {
	pub depth: AllocatedImage,
}

impl GBuffers {
	pub(super) fn new(
		extent: vk::Extent2D,
		allocator: Arc<Allocator>,
		device: Arc<Device>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let gbuffers = (0..frame_count)
			.map(|_| GBuffer::new(extent, allocator.clone(), device.clone()))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self { gbuffers })
	}

	pub(super) fn resize(
		&mut self,
		extent: vk::Extent2D,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		tracing::info!("resize gbuffers with {:?}", extent);
		for gbuffer in &mut self.gbuffers {
			gbuffer.resize(extent)?;
		}
		Ok(())
	}
}

impl GBuffer {
	fn new(
		extent: vk::Extent2D,
		allocator: Arc<Allocator>,
		device: Arc<Device>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let depth = AllocatedImage::new(
			vk::Format::D32_SFLOAT,
			extent,
			vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
			vk::ImageAspectFlags::DEPTH,
			vk_mem::AllocationCreateFlags::DEDICATED_MEMORY,
			allocator,
			device
		)?;
		Ok(Self {
			depth
		})
	}

	fn resize(
		&mut self,
		extent: vk::Extent2D,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		self.depth.resize(extent)?;
		Ok(())
	}
}