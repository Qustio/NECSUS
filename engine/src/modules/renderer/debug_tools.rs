use std::{
	error::Error,
	sync::{Arc, atomic::AtomicBool},
};

use ash::{vk::Extent2D, *};
use shipyard::Unique;
use vk_mem::Alloc;

use super::{
	buffer::Buffer,
	vulkan_context::{Allocator, Device},
};

#[derive(Unique)]
pub struct FrameCapture {
	pub(super) pending: Arc<AtomicBool>,
	pub(super) buffer: Buffer<u8>,
	pub(super) extent: vk::Extent2D,
	pub(super) image: (vk::Image, vk_mem::Allocation),
	device: Arc<Device>,
	allocator: Arc<Allocator>,
}

impl FrameCapture {
	pub(super) fn new(
		device: Arc<Device>,
		allocator: Arc<Allocator>,
		format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let extent = vk::Extent2D {
			width: 960,
			height: 540,
		};
		let buffer = Buffer::new(
			allocator.clone(),
			(extent.height * extent.width * 4) as u64,
			vk::BufferUsageFlags::TRANSFER_DST,
			vk_mem::MemoryUsage::AutoPreferHost,
			vk_mem::AllocationCreateFlags::MAPPED
				| vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
		)?;
		//todo delete
		let image = unsafe {
			allocator.create_image(
				&vk::ImageCreateInfo::default()
					.image_type(vk::ImageType::TYPE_2D)
					.extent(
						vk::Extent3D::default()
							.width(extent.width)
							.height(extent.height)
							.depth(1),
					)
					.usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
					.sharing_mode(vk::SharingMode::EXCLUSIVE)
					.samples(vk::SampleCountFlags::TYPE_1)
					.mip_levels(1)
					.array_layers(1)
					.initial_layout(vk::ImageLayout::UNDEFINED)
					.format(format),
				&vk_mem::AllocationCreateInfo {
					usage: vk_mem::MemoryUsage::AutoPreferDevice,
					..Default::default()
				},
			)?
		};
		Ok(Self {
			pending: Arc::new(AtomicBool::new(false)),
			buffer,
			extent,
			image,
			device,
			allocator,
		})
	}

	pub(super) fn copy(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
		let info = self.buffer.allocation_info();
		let image = unsafe {
			std::slice::from_raw_parts(info.mapped_data as *const u8, info.size as usize)
		};
		tracy_client::frame_image(
			image,
			self.extent.width as u16,
			self.extent.height as u16,
			0,
			false,
		);
		Ok(())
	}
}

impl Drop for FrameCapture {
	fn drop(&mut self) {
		unsafe {
			self.allocator
				.destroy_image(self.image.0, &mut self.image.1);
		}
	}
}

//tracy_client::frame_image(image, width, height, offset, flip);
