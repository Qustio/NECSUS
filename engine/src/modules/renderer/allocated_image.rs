use std::{error::Error, sync::Arc};

use ash::*;
use vk_mem::Alloc;

use super::vulkan_context::{Allocator, Device};

pub(super) struct AllocatedImage {
	pub(super) image: vk::Image,
	pub(super) view: vk::ImageView,
	layer_views: Vec<vk::ImageView>,
	format: vk::Format,
	array_layers: u32,
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
		Self::new_array(
			format,
			extent,
			1,
			usage,
			aspect_mask,
			flags,
			allocator,
			device,
		)
	}

	/// `array_layers > 1` additionally allocates one single-layer `TYPE_2D` view per
	/// layer (see `layer_view`), for use as an individual dynamic-rendering render
	/// target; `view` itself becomes `TYPE_2D_ARRAY` for sampling the whole array.
	pub(super) fn new_array(
		format: vk::Format,
		extent: vk::Extent2D,
		array_layers: u32,
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
					.extent(vk::Extent3D {
						width: extent.width,
						height: extent.height,
						depth: 1,
					})
					.usage(usage)
					.sharing_mode(vk::SharingMode::EXCLUSIVE)
					.samples(vk::SampleCountFlags::TYPE_1)
					.mip_levels(1)
					.array_layers(array_layers)
					.initial_layout(vk::ImageLayout::UNDEFINED)
					.format(format),
				&create_info,
			)?
		};
		let main_view_type = if array_layers > 1 {
			vk::ImageViewType::TYPE_2D_ARRAY
		} else {
			vk::ImageViewType::TYPE_2D
		};
		let view = unsafe {
			device.create_image_view(
				&vk::ImageViewCreateInfo::default()
					.image(image)
					.view_type(main_view_type)
					.format(format)
					.subresource_range(
						vk::ImageSubresourceRange::default()
							.aspect_mask(aspect_mask)
							.layer_count(array_layers)
							.level_count(1),
					),
				None,
			)?
		};
		let layer_views = if array_layers > 1 {
			(0..array_layers)
				.map(|layer| unsafe {
					device.create_image_view(
						&vk::ImageViewCreateInfo::default()
							.image(image)
							.view_type(vk::ImageViewType::TYPE_2D)
							.format(format)
							.subresource_range(
								vk::ImageSubresourceRange::default()
									.aspect_mask(aspect_mask)
									.base_array_layer(layer)
									.layer_count(1)
									.level_count(1),
							),
						None,
					)
				})
				.collect::<Result<Vec<_>, _>>()?
		} else {
			Vec::new()
		};
		Ok(Self {
			image,
			view,
			layer_views,
			format,
			array_layers,
			usage,
			aspect_mask,
			flags,
			allocation,
			allocator,
			device,
		})
	}

	/// Single-layer render target for layer `layer`. For a non-array image (created
	/// via `new`), `layer` is ignored and the main view is returned.
	pub(super) fn layer_view(&self, layer: u32) -> vk::ImageView {
		if self.array_layers > 1 {
			self.layer_views[layer as usize]
		} else {
			self.view
		}
	}

	pub(super) fn resize(
		&mut self,
		extent: vk::Extent2D,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		*self = Self::new_array(
			self.format,
			extent,
			self.array_layers,
			self.usage,
			self.aspect_mask,
			self.flags,
			self.allocator.clone(),
			self.device.clone(),
		)?;
		Ok(())
	}
}

impl Drop for AllocatedImage {
	fn drop(&mut self) {
		unsafe {
			for view in &self.layer_views {
				self.device.destroy_image_view(*view, None);
			}
			self.device.destroy_image_view(self.view, None);
			self.allocator
				.destroy_image(self.image, &mut self.allocation);
		}
	}
}
