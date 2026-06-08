use std::{default::Default, error::Error, sync::Arc};

use super::frame_sync::FrameSync;
use super::vulkan_context::{Device, Instance, Surface};
use ash::*;
use shipyard::Unique;
use winit::dpi::PhysicalSize;

#[derive(Unique)]
pub struct Swapchain {
	swapchain_loader: khr::swapchain::Device,
	swapchain: vk::SwapchainKHR,
	pub(super) images: Vec<vk::Image>,
	pub(super) image_views: Vec<vk::ImageView>,
	pub(super) format: vk::SurfaceFormatKHR,
	pub(super) extent: vk::Extent2D,
	present_mode: vk::PresentModeKHR,
	pub frame_count: u32,
	surface: Arc<Surface>,
	device: Arc<Device>,
	instance: Arc<Instance>
}

impl Swapchain {
	pub(super) fn new(
		instance: Arc<Instance>,
		device: Arc<Device>,
		surface: Arc<Surface>,
		size: PhysicalSize<u32>,
		old_swapchain: Option<vk::SwapchainKHR>
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

		let capabilities = unsafe {
			surface
				.get_physical_device_surface_capabilities(device.physical_device, surface.surface)?
		};
		let min_frames = capabilities.min_image_count.max(2);

		let formats = unsafe {
			surface.get_physical_device_surface_formats(device.physical_device, surface.surface)?
		};
		tracing::debug!("image_formats: {:#?}", formats);
		let format = formats
			.into_iter()
			.min_by_key(|&f| match f.format {
				vk::Format::R8G8B8A8_UNORM => 1,
				vk::Format::R8G8B8A8_SRGB => 0,
				_ => 2,
			})
			.ok_or("no suitable formats found")?;
		tracing::debug!("selected image_formats: {:#?}", format);

		let present_modes = unsafe {
			surface.get_physical_device_surface_present_modes(
				device.physical_device,
				surface.surface,
			)?
		};
		tracing::debug!("present_modes: {:?}", present_modes);
		let present_mode = present_modes
			.into_iter()
			.min_by_key(|&pm| match pm {
				vk::PresentModeKHR::FIFO_RELAXED => 2,
				vk::PresentModeKHR::FIFO => 1,
				vk::PresentModeKHR::MAILBOX => 0,
				_ => 3,
			})
			.ok_or("no suitable present mode found")?;
		tracing::debug!("selected present_mode: {:?}", present_mode);

		tracing::info!("current: {:?}", capabilities.current_extent);
		tracing::info!("max: {:?}", capabilities.max_image_extent);
		tracing::info!("min: {:?}", capabilities.min_image_extent);
		let extent = capabilities.current_extent;
		tracing::info!("extent: {:?}", extent);

		let old_swapchain = old_swapchain.unwrap_or(vk::SwapchainKHR::null());
		let swapchain = unsafe {
			swapchain_loader.create_swapchain(
				&vk::SwapchainCreateInfoKHR::default()
					.surface(surface.surface)
					.min_image_count(min_frames)
					.image_format(format.format)
					.image_color_space(format.color_space)
					.image_extent(extent)
					.image_array_layers(1)
					.image_usage(
						vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
					)
					.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
					.pre_transform(capabilities.current_transform)
					.composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
					.present_mode(present_mode)
					.old_swapchain(old_swapchain)
					.clipped(true),
				None,
			)?
		};

		let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };

		let image_views = images
			.iter()
			.map(|&image| unsafe {
				device.create_image_view(
					&vk::ImageViewCreateInfo {
						image,
						view_type: vk::ImageViewType::TYPE_2D,
						format: format.format,
						subresource_range: vk::ImageSubresourceRange {
							aspect_mask: vk::ImageAspectFlags::COLOR,
							level_count: 1,
							layer_count: 1,
							..Default::default()
						},
						..Default::default()
					},
					None,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;

		let frame_count = images.len() as u32;

		Ok(Self {
			swapchain_loader,
			swapchain,
			images,
			image_views,
			format,
			extent,
			present_mode,
			frame_count,
			surface,
			device,
			instance,
		})
	}

	pub(super) fn acqure(
		&self,
		frame_sync: &mut FrameSync,
	) -> Result<bool, Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		let sem_idx = frame_sync.acquire_sem_idx as usize;
		let (id, subopt) = unsafe {
			self.swapchain_loader.acquire_next_image(
				self.swapchain,
				u64::MAX,
				frame_sync.acquire_semaphores[sem_idx],
				vk::Fence::null(),
			)?
		};
		// bind semaphore to acquired image slot; old slot semaphore returns to pool
		std::mem::swap(
			&mut frame_sync.image_availabe[id as usize],
			&mut frame_sync.acquire_semaphores[sem_idx],
		);
		frame_sync.acquire_sem_idx = (frame_sync.acquire_sem_idx + 1) % self.frame_count;
		frame_sync.acquired_image_index = id;
		tracy_client::plot!("frame_id", frame_sync.frame_id as f64);
		tracy_client::plot!("acquired_image_index", frame_sync.acquired_image_index as f64);
		Ok(subopt)
	}

	pub(super) fn recreate(
		&mut self,
		size: PhysicalSize<u32>,
	) -> Result<vk::Extent2D, Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		self.device.wait()?;
		*self = Self::new(
			self.instance.clone(),
			self.device.clone(),
			self.surface.clone(),
			size,
			Some(self.swapchain)
		)?;
		Ok(self.extent)
	}

	pub(super) fn present(
		&self,
		frame_sync: &mut FrameSync,
	) -> Result<bool, Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		let img = frame_sync.acquired_image_index as usize;
		let queue = self
			.device
			.graphics_queue
			.lock()
			.expect("couldnt lock queue");
		let subopt = unsafe {
			self.swapchain_loader.queue_present(
				*queue,
				&vk::PresentInfoKHR::default()
					.wait_semaphores(&[frame_sync.render_finished[img]])
					.image_indices(&[frame_sync.acquired_image_index])
					.swapchains(&[self.swapchain]),
			)?
		};
		frame_sync.frame_id = (frame_sync.frame_id + 1) % self.frame_count;
		Ok(subopt)
	}
}

impl Drop for Swapchain {
	fn drop(&mut self) {
		unsafe {
			let _ = self.device.wait_queue();
			for image in &self.image_views {
				self.device.destroy_image_view(*image, None);
			}
			self.swapchain_loader
				.destroy_swapchain(self.swapchain, None);
		}
	}
}
