use std::{error::Error, sync::Arc};

use ash::*;
use vk_mem::Alloc;

use super::vulkan_context::Allocator;


pub struct Buffer {
	buffer: vk::Buffer,
	allocation: vk_mem::Allocation,
	allocator: Arc<Allocator>
}

impl Buffer {
	pub(super) fn new(
		allocator: Arc<Allocator>,
		size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
		memory_usage: vk_mem::MemoryUsage,
        flags: vk_mem::AllocationCreateFlags,
	)  -> Result<Self, Box<dyn Error + Send + Sync>> {
		let (buffer, allocation) = unsafe {
			allocator.create_buffer(
				&vk::BufferCreateInfo::default()
					.size(size)
					.usage(usage)
					.sharing_mode(vk::SharingMode::EXCLUSIVE),
				&vk_mem::AllocationCreateInfo {
					flags,
					usage: memory_usage,
					..Default::default()
				}
			)?
		};
		Ok(Self{
			buffer,
			allocation,
			allocator
		})
	}

	pub(super) fn from_slice<T: bytemuck::Pod>(
		allocator: Arc<Allocator>,
		data: &[T],
        usage: vk::BufferUsageFlags,
	)  -> Result<Self, Box<dyn Error + Send + Sync>> {
		let buf = Self::new(
			allocator,
			size_of_val(data) as vk::DeviceSize,
			usage,
			vk_mem::MemoryUsage::AutoPreferHost,
			vk_mem::AllocationCreateFlags::MAPPED | vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
		)?;
		unsafe {
			let info = buf.allocator.get_allocation_info(&buf.allocation);
			std::ptr::copy_nonoverlapping::<T>(data.as_ptr(), info.mapped_data as *mut T, data.len());
		}
		Ok(buf)
	}

	pub(super) fn buffer(&self) -> vk::Buffer {
		self.buffer
	}
}

impl Drop for Buffer {
	fn drop(&mut self) {
		unsafe {
			self.allocator.destroy_buffer(self.buffer, &mut self.allocation);
		}
	}
}