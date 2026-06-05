use std::{error::Error, sync::Arc, default::Default};

use super::command_context::CommandContext;
use super::frame_sync::{FrameSync};
use super::vulkan_context::{Instance, Device, Surface};
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
}

impl Swapchain {
    pub(super) fn new(
        instance: Arc<Instance>,
        device: Arc<Device>,
        surface: Arc<Surface>,
        size: PhysicalSize<u32>
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

        let capabilities = unsafe {
            surface.get_physical_device_surface_capabilities(device.physical_device, surface.surface)?
        };
        let min_frames = capabilities.min_image_count.max(2);

        let formats = unsafe {
            surface.get_physical_device_surface_formats(device.physical_device, surface.surface)?
        };
        tracing::debug!("image_formats: {:#?}", formats);
        let format = formats.into_iter()
            .min_by_key(|&f| {
                match f.format {
					vk::Format::R8G8B8A8_UNORM => 1,
                    vk::Format::R8G8B8A8_SRGB => 0,
                    _ => 2
                }
            })
            .ok_or("no suitable formats found")?;
        tracing::debug!("selected image_formats: {:#?}", format);

        let present_modes = unsafe {
            surface.get_physical_device_surface_present_modes(device.physical_device, surface.surface)?
        };
        tracing::debug!("present_modes: {:?}", present_modes);
        let present_mode = present_modes.into_iter()
            .min_by_key(|&pm| {
                match pm {
                    vk::PresentModeKHR::FIFO_RELAXED => 0,
                    vk::PresentModeKHR::FIFO => 1,
                    vk::PresentModeKHR::MAILBOX => 2,
                    _ => 3,
                }
            })
            .ok_or("no suitable present mode found")?;
        tracing::debug!("selected present_mode: {:?}", present_mode);

        let extent = vk::Extent2D{
            width: size.width,
            height: size.height,
        };

        let swapchain = unsafe {
            swapchain_loader.create_swapchain(
                &vk::SwapchainCreateInfoKHR::default()
                    .surface(surface.surface)
                    .min_image_count(min_frames)
                    .image_format(format.format)
                    .image_color_space(format.color_space)
                    .image_extent(extent)
                    .image_array_layers(1)
                    .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(capabilities.current_transform)
                    .composite_alpha(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED)
                    .present_mode(present_mode)
                    .clipped(true),
                None
            )?
        };

        let images = unsafe {
            swapchain_loader.get_swapchain_images(swapchain)?
        };

        let image_views = images.iter()
            .map(|&image| unsafe {
                device.create_image_view(
                    &vk::ImageViewCreateInfo{
                        image,
                        view_type: vk::ImageViewType::TYPE_2D,
                        format: format.format,
                        subresource_range: vk::ImageSubresourceRange{
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            level_count: 1,
                            layer_count: 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    None
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let frame_count = images.len() as u32;

        Ok(Self{
            swapchain_loader,
            swapchain,
            images,
            image_views,
            format,
            extent,
            present_mode,
            frame_count,
            surface,
            device
        })
    }

	pub(super) fn acqure (
		&self,
		frame_sync: &mut FrameSync
	) -> Result<bool, Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		let (id, subopt) = unsafe {
			self.swapchain_loader.acquire_next_image(
				self.swapchain,
				u64::MAX,
				frame_sync.image_availabe[frame_sync.frame_id as usize],
				vk::Fence::null()
			)?
		};
		frame_sync.acquired_image_index = id;
		Ok(subopt)
	}

	pub(super) fn recreate(
		&mut self,
		size: PhysicalSize<u32>,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		self.device.wait()?;
		let extent = vk::Extent2D { width: size.width, height: size.height };
		let old_swapchain = self.swapchain;
		unsafe {
			for &view in &self.image_views{
				self.device.destroy_image_view(view, None);
			}
			let capabilities = self.surface.get_physical_device_surface_capabilities(
				self.device.physical_device, self.surface.surface
			)?;
			let new_swapchain = 
			self.swapchain_loader.create_swapchain(
				&vk::SwapchainCreateInfoKHR::default()
					.surface(self.surface.surface)
					.min_image_count(self.frame_count)
					.image_format(self.format.format)
					.image_color_space(self.format.color_space)
					.image_extent(extent)
					.image_array_layers(1)
					.image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
					.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
					.pre_transform(capabilities.current_transform)
					.composite_alpha(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED)
					.present_mode(self.present_mode)
					.clipped(true)
					.old_swapchain(old_swapchain),  // ← key difference from new()
				None
			)?;
			self.swapchain_loader.destroy_swapchain(old_swapchain, None);
			self.swapchain = new_swapchain;
			self.images = unsafe { self.swapchain_loader.get_swapchain_images(new_swapchain)? };
			self.image_views = self.images.iter()
				.map(|&image| {
					self.device.create_image_view(&vk::ImageViewCreateInfo {
						image,
						view_type: vk::ImageViewType::TYPE_2D,
						format: self.format.format,
						subresource_range: vk::ImageSubresourceRange {
							aspect_mask: vk::ImageAspectFlags::COLOR,
							level_count: 1,
							layer_count: 1,
							..Default::default()
						},
						..Default::default()
					}, None)
				})
				.collect::<Result<Vec<_>, _>>()?;
			self.extent = extent;
			self.frame_count = self.images.len() as u32;
		}

		Ok(())
	}


	pub(super) fn present (
		&self,
		frame_sync: &mut FrameSync
	) -> Result<bool, Box<dyn Error + Send + Sync>> {
		let _zone = tracy_client::span!();
		let frame = frame_sync.frame_id as usize;
		let queue = self.device.graphics_queue
				.lock()
				.expect("couldnt lock queue");
		let subopt= unsafe {
			self.swapchain_loader.queue_present(
				*queue,
				&vk::PresentInfoKHR::default()
					.wait_semaphores(&[frame_sync.render_finished[frame]])
					.image_indices(&[frame_sync.acquired_image_index])
					.swapchains(&[self.swapchain])
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
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
        }
    }
}