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
	pub depth_format: vk::Format,
	pub shadow_format: vk::Format,
	pub shadow_resolution: u32,
}

pub(super) struct GBuffer {
	pub depth: AllocatedImage,
	pub shadow: AllocatedImage,
}

impl GBuffers {
	pub(super) fn new(
		extent: vk::Extent2D,
		allocator: Arc<Allocator>,
		device: Arc<Device>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let depth_format = vk::Format::D32_SFLOAT;
		let shadow_format = vk::Format::D32_SFLOAT;
		let shadow_resolution = 1024;
		let gbuffers = (0..frame_count)
			.map(|_| {
				GBuffer::new(
					depth_format,
					shadow_format,
					extent,
					shadow_resolution,
					allocator.clone(),
					device.clone()
				)
		}).collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			gbuffers,
    		depth_format,
			shadow_format,
			shadow_resolution
		})
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
		depth_format: vk::Format,
		shadow_format: vk::Format,
		extent: vk::Extent2D,
		shadow_resolution: u32,
		allocator: Arc<Allocator>,
		device: Arc<Device>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let depth = AllocatedImage::new(
			depth_format,
			extent,
			vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
			vk::ImageAspectFlags::DEPTH,
			vk_mem::AllocationCreateFlags::DEDICATED_MEMORY,
			allocator.clone(),
			device.clone()
		)?;
		let shadow = AllocatedImage::new(
			depth_format,
			vk::Extent2D{
				width: shadow_resolution,
				height: shadow_resolution,
			},
			vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
			vk::ImageAspectFlags::DEPTH,
			vk_mem::AllocationCreateFlags::DEDICATED_MEMORY,
			allocator.clone(),
			device.clone()
		)?;
		Ok(Self {
			depth,
			shadow
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