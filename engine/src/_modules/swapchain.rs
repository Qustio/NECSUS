use std::{error::Error, sync::Arc};

use shipyard::Unique;
use vulkano::{format::Format, image::{Image, ImageAspects, ImageCreateInfo, ImageLayout, ImageSubresourceRange, ImageUsage, view::{ImageView, ImageViewCreateInfo, ImageViewType}}, memory::allocator::AllocationCreateInfo, swapchain::{SurfaceInfo, SwapchainCreateInfo}};
use winit::dpi::PhysicalSize;
use vulkano::swapchain::Swapchain as VkSwapchain;

use crate::modules;

#[derive(Unique)]
pub struct Swapchain {
    pub(crate) swapchain: Arc<VkSwapchain>,
    pub(crate) images: Vec<Arc<Image>>,
    pub(crate) image_views: Vec<Arc<ImageView>>,
    pub(crate) depth_images: Vec<Arc<Image>>,
    pub(crate) depth_image_views: Vec<Arc<ImageView>>,
    pub(crate) extent: [u32; 2],
	pub(crate) frames_count: u32,
}

impl Swapchain {
    pub fn new(
        vulkan_context: &modules::vulkan_context::VulkanContext,
        size: PhysicalSize<u32>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        tracing::debug!("Swapchain::new");
        tracing::debug!("{size:?}");

        let surface_info = SurfaceInfo::default();

        // Get surface capatabilities
        let capabilities =
            vulkan_context.physical_device.surface_capabilities(&vulkan_context.surface, &surface_info)?;

        // Get surface formats
        let formats =
            vulkan_context.physical_device.surface_formats(&vulkan_context.surface, &surface_info)?;
		tracing::debug!("image_formats: {:?}", formats);

        // Get supported present modes
        let present_modes =
            vulkan_context.physical_device.surface_present_modes(&vulkan_context.surface, &surface_info)?;
        tracing::debug!("present_modes: {:?}", present_modes);

        let frames = capabilities.min_image_count.max(2);

        let (swapchain, images) = VkSwapchain::new(
            &vulkan_context.device,
            &vulkan_context.surface,
            &SwapchainCreateInfo{
                min_image_count: frames,
                image_format: formats[0].0,
                image_extent: size.into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                //present_modes: &[PresentMode::Immediate, PresentMode::Mailbox, PresentMode::Fifo, PresentMode::FifoRelaxed],
                //present_modes: &present_modes,
                present_mode: vulkano::swapchain::PresentMode::Mailbox,
                composite_alpha: capabilities.supported_composite_alpha.into_iter().next().unwrap(),
                ..Default::default()
            }
        )?;

		// Get the ACTUAL image count
		let frames_count = swapchain.image_count();

        let image_views = images
            .iter()
            .map(|image| {
                ImageView::new(image, &ImageViewCreateInfo{
					subresource_range: ImageSubresourceRange{
						aspects: ImageAspects::COLOR,
						..Default::default()
					},
					format: images[0].format(),
					..Default::default()
				}).unwrap()
            })
            .collect::<Vec<_>>();

        // Create images and image views for depth
        let (depth_images, depth_image_views) = (0..frames_count)
            .map(|_| {
                let depth_image = Image::new(
                    &vulkan_context.allocator,
                    &ImageCreateInfo {
                        initial_layout: ImageLayout::Undefined,
                        usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
						format: Format::D32_SFLOAT_S8_UINT,
                        extent: [size.width, size.height, 1],
                        ..Default::default()
                    },
                    &AllocationCreateInfo {
                        ..Default::default()
                    }
                ).unwrap();
                let depth_image_view = ImageView::new(
                    &depth_image,
                    &ImageViewCreateInfo {
                        format: Format::D32_SFLOAT_S8_UINT,
                        view_type: ImageViewType::Dim2d,
                        subresource_range: ImageSubresourceRange{
                            aspects: ImageAspects::DEPTH,
                            base_mip_level: 0,
                            level_count: 1.into(),
                            base_array_layer: 0,
                            layer_count: 1.into(),
                        },
                        //usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
                        ..Default::default()
                    }
                ).unwrap();
                (depth_image, depth_image_view)
            })
            .collect::<(Vec<_>, Vec<_>)>();

        // Create render_pass
        // let render_pass = {
        //     let col_attachment_description = vk::AttachmentDescription::default()
        //         .format(format.format)
        //         .samples(vk::SampleCountFlags::TYPE_1)
        //         .load_op(vk::AttachmentLoadOp::CLEAR)
        //         .store_op(vk::AttachmentStoreOp::STORE)
        //         .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        //         .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        //         .initial_layout(vk::ImageLayout::UNDEFINED)
        //         .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        //     let depth_attachment_description = vk::AttachmentDescription::default()
        //         .format(vk::Format::D32_SFLOAT)
        //         .samples(vk::SampleCountFlags::TYPE_1)
        //         .load_op(vk::AttachmentLoadOp::CLEAR)
        //         .store_op(vk::AttachmentStoreOp::STORE)
        //         .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        //         .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        //         .initial_layout(vk::ImageLayout::UNDEFINED)
        //         .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        //     let attachments = [col_attachment_description, depth_attachment_description];

        //     let color_attachment_ref = vk::AttachmentReference::default()
        //         .attachment(0)
        //         .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

        //     let depth_attachment_ref = vk::AttachmentReference::default()
        //         .attachment(1)
        //         .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        //     let color_attachments = [color_attachment_ref];

        //     let subpass = vk::SubpassDescription::default()
        //         .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        //         .color_attachments(&color_attachments)
        //         .depth_stencil_attachment(&depth_attachment_ref);

        //     let subpasses = [subpass];

        //     let subpass_dependency = vk::SubpassDependency::default()
        //         .src_subpass(vk::SUBPASS_EXTERNAL)
        //         .dst_subpass(0)
        //         .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        //         .src_access_mask(vk::AccessFlags::empty())
        //         .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        //         .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        //     let dependecies = [subpass_dependency];

        //     let renderpass_info = vk::RenderPassCreateInfo::default()
        //         .attachments(&attachments)
        //         .subpasses(&subpasses)
        //         .dependencies(&dependecies);

        //     unsafe { device.create_render_pass(&renderpass_info, None)? }
        // };

        // Create framebuffers
        // let framebuffers = Self::create_framebuffers(
        //     &device,
        //     &image_views,
        //     &depth_image_views,
        //     render_pass,
        //     extent,
        // )?;
        let extent = size.into();
        Ok(Self {
            swapchain,
            images,
            image_views,
            depth_images,
            depth_image_views,
            extent,
			frames_count,
        })
    }

	pub(crate) fn recreate(
        &mut self,
        vulkan_context: &modules::vulkan_context::VulkanContext,
		size: PhysicalSize<u32>,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		tracing::debug!("Swapchain::recreate");
		let (swapchain, images) =  self.swapchain.recreate(
			&SwapchainCreateInfo{
				image_extent: size.into(),
                image_format: self.swapchain.image_format(),
                image_usage: self.swapchain.image_usage(),
                composite_alpha: self.swapchain.composite_alpha(),
                present_mode: self.swapchain.present_mode(),
                min_image_count: self.swapchain.image_count(),
				..Default::default()
			}
		)?;
		self.swapchain = swapchain;
		self.images = images;
		self.extent = size.into();
		self.frames_count = self.swapchain.image_count();


        self.image_views = self.images
            .iter()
            .map(|image| {
                ImageView::new(&image, &ImageViewCreateInfo{
					subresource_range: ImageSubresourceRange{
						aspects: ImageAspects::COLOR,
						..Default::default()
					},
					format: self.images[0].format(),
					..Default::default()
				}).unwrap()
            })
            .collect::<Vec<_>>();

        let (depth_images, depth_image_views) = (0..self.frames_count)
            .map(|_| {
                let depth_image = Image::new(
                    &vulkan_context.allocator,
                    &ImageCreateInfo {
                        initial_layout: ImageLayout::Undefined,
                        usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
						format: Format::D32_SFLOAT_S8_UINT,
                        extent: [size.width, size.height, 1],
                        ..Default::default()
                    },
                    &AllocationCreateInfo {
                        ..Default::default()
                    }
                ).unwrap();
                let depth_image_view = ImageView::new(
                    &depth_image,
                    &ImageViewCreateInfo {
                        format: Format::D32_SFLOAT_S8_UINT,
                        view_type: ImageViewType::Dim2d,
                        subresource_range: ImageSubresourceRange{
                            aspects: ImageAspects::DEPTH,
                            base_mip_level: 0,
                            level_count: 1.into(),
                            base_array_layer: 0,
                            layer_count: 1.into(),
                        },
                        //usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
                        ..Default::default()
                    }
                ).unwrap();
                (depth_image, depth_image_view)
            })
            .collect::<(Vec<_>, Vec<_>)>();
        
        self.depth_images = depth_images;
        self.depth_image_views = depth_image_views;

		Ok(())
	}
}