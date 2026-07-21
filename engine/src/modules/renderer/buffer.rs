use std::{error::Error, marker::PhantomData, sync::Arc};

use ash::*;
use vk_mem::Alloc;

use super::vulkan_context::Allocator;

pub struct Buffer<T> {
	buffer: vk::Buffer,
	allocation: vk_mem::Allocation,
	allocator: Arc<Allocator>,
	_type: PhantomData<T>,
}

impl<T> Buffer<T> {
	pub(super) fn new(
		allocator: Arc<Allocator>,
		size: vk::DeviceSize,
		usage: vk::BufferUsageFlags,
		memory_usage: vk_mem::MemoryUsage,
		flags: vk_mem::AllocationCreateFlags,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
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
				},
			)?
		};
		Ok(Self {
			buffer,
			allocation,
			allocator,
			_type: PhantomData,
		})
	}

	pub(super) fn from_slice(
		allocator: Arc<Allocator>,
		data: &[T],
		usage: vk::BufferUsageFlags,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let buf = Self::new(
			allocator,
			size_of_val(data) as vk::DeviceSize,
			usage,
			vk_mem::MemoryUsage::AutoPreferHost,
			vk_mem::AllocationCreateFlags::MAPPED
				| vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
		)?;
		unsafe {
			let info = buf.allocator.get_allocation_info(&buf.allocation);
			std::ptr::copy_nonoverlapping::<T>(
				data.as_ptr(),
				info.mapped_data as *mut T,
				data.len(),
			);
		}
		Ok(buf)
	}

	pub(super) fn new_uniform(
		allocator: Arc<Allocator>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		Self::new(
			allocator,
			size_of::<T>() as vk::DeviceSize,
			vk::BufferUsageFlags::UNIFORM_BUFFER,
			vk_mem::MemoryUsage::AutoPreferHost,
			vk_mem::AllocationCreateFlags::MAPPED
				| vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
		)
	}

	pub(super) fn write(&self, data: &T)
	where
		T: Copy,
	{
		unsafe {
			let info = self.allocation_info();
			std::ptr::copy_nonoverlapping::<T>(data as *const T, info.mapped_data as *mut T, 1);
		}
	}

	pub(super) fn new_storage(
		allocator: Arc<Allocator>,
		capacity: usize,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		Self::new(
			allocator,
			(capacity * size_of::<T>()) as vk::DeviceSize,
			vk::BufferUsageFlags::STORAGE_BUFFER,
			vk_mem::MemoryUsage::AutoPreferHost,
			vk_mem::AllocationCreateFlags::MAPPED
				| vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
		)
	}

	pub(super) fn write_slice(&self, data: &[T])
	where
		T: Copy,
	{
		unsafe {
			let info = self.allocation_info();
			std::ptr::copy_nonoverlapping::<T>(
				data.as_ptr(),
				info.mapped_data as *mut T,
				data.len(),
			);
		}
	}

	pub(super) fn buffer(&self) -> vk::Buffer {
		self.buffer
	}

	pub(super) fn allocation_info(&self) -> vk_mem::AllocationInfo {
		self.allocator.get_allocation_info(&self.allocation)
	}
}

impl<T> Drop for Buffer<T> {
	fn drop(&mut self) {
		unsafe {
			self.allocator
				.destroy_buffer(self.buffer, &mut self.allocation);
		}
	}
}
