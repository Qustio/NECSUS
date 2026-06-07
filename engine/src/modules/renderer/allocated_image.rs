use std::{error::Error, sync::Arc};

use ash::*;
use vk_mem::Alloc;

use super::vulkan_context::{Allocator, Device};

pub(super) struct AllocatedImage {
	pub(super) image: vk::Image,
	pub(super) view: vk::ImageView,
	format: vk::Format,
	usage: vk::ImageUsageFlags,
	aspect_mask: vk::ImageAspectFlags,
	flags: vk_mem::AllocationCreateFlags,
	allocation: vk_mem::Allocation,
	allocator: Arc<Allocator>,
	device: Arc<Device>,
}

impl AllocatedImage {
	pub(super) fn new(
		format: vk::Format,
		extent: vk::Extent2D,
		usage: vk::ImageUsageFlags,
		aspect_mask: vk::ImageAspectFlags,
		flags: vk_mem::AllocationCreateFlags,
		allocator: Arc<Allocator>,
		device: Arc<Device>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let create_info = vk_mem::AllocationCreateInfo {
			flags,
			..Default::default()
		};
		let (image, allocation) = unsafe { 
			allocator.create_image(
				&vk::ImageCreateInfo::default()
					.image_type(vk::ImageType::TYPE_2D)
					.extent(
						vk::Extent3D{
							width: extent.width,
							height: extent.height,
							depth: 1,
						},
					)
					.usage(usage)
					.sharing_mode(vk::SharingMode::EXCLUSIVE)
					.samples(vk::SampleCountFlags::TYPE_1)
					.mip_levels(1)
					.array_layers(1)
					.initial_layout(vk::ImageLayout::UNDEFINED)
					.format(format),
				&create_info
			)? 
		};
		let view = unsafe { 
			device.create_image_view(
				&vk::ImageViewCreateInfo::default()
					.image(image)
					.view_type(vk::ImageViewType::TYPE_2D)
					.format(format)
					.subresource_range(vk::ImageSubresourceRange::default()
						.aspect_mask(aspect_mask)
						.layer_count(1)
						.level_count(1)
					),
				None
			)?
		};
		Ok(Self {
			image,
			view,
			format,
			usage,
			aspect_mask,
			flags,
			allocation,
			allocator,
			device,
		})
	}

	pub(super) fn resize(
		&mut self,
		extent: vk::Extent2D,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		*self = Self::new(
			self.format,
			extent,
			self.usage,
			self.aspect_mask,
			self.flags,
			self.allocator.clone(),
			self.device.clone()
		)?;
		Ok(())
	}
}

impl Drop for AllocatedImage {
	fn drop(&mut self) {
		unsafe {
			self.device.destroy_image_view(self.view, None);
			self.allocator
				.destroy_image(self.image, &mut self.allocation);
		}
	}
}
