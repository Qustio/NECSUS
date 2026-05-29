use std::{error::Error, sync::Arc, default::Default};

use super::vulkan_context::{Instance, Device, Surface};
use ash::*;
use shipyard::Unique;
use winit::dpi::PhysicalSize;

#[derive(Unique)]
pub struct Swapchain {
    swapchain_loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    present_mode: vk::PresentModeKHR,
    device: Arc<Device>
}

impl Swapchain {
    pub(super) fn new(
        instance: Arc<Instance>,
        device: Arc<Device>,
        surface: &Surface,
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
        tracing::debug!("image_formats: {:?}", formats);
        let format = formats.into_iter()
            .min_by_key(|&f| {
                match f.format {
                    vk::Format::R8G8B8A8_SRGB => 0,
                    vk::Format::R8G8B8A8_UNORM => 1,
                    _ => 2
                }
            })
            .ok_or("no suitable formats found")?;
        tracing::debug!("selected image_formats: {:?}", format);

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
                    .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                    .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(capabilities.current_transform)
                    .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
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
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    None
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self{
            swapchain_loader,
            swapchain,
            images,
            image_views,
            format,
            extent,
            present_mode,
            device
        })
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for image in &self.image_views {
                self.device.destroy_image_view(*image, None);
            }
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
        }
    }
}