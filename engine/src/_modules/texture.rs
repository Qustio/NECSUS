use std::{error::Error, io::BufReader, sync::Arc};

use shipyard::Unique;
use vulkano::{buffer::{Buffer, BufferCreateInfo, BufferUsage}, command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferToImageInfo, PrimaryCommandBufferAbstract}, device::{DeviceOwnedVulkanObject, Queue}, format::Format, image::{Image, ImageAspect, ImageAspects, ImageCreateInfo, ImageLayout, ImageSubresourceRange, ImageTiling, ImageType, ImageUsage, SampleCount, sampler::{BorderColor, Filter, Sampler, SamplerAddressMode, SamplerCreateInfo, SamplerMipmapMode}, view::{ImageView, ImageViewCreateInfo, ImageViewType}}, memory::allocator::{AllocationCreateInfo, MemoryTypeFilter}};

use crate::modules::*;

#[derive(Unique)]
pub(super) struct Texture {
    pub(super) image: Arc<Image>,
    pub(super) image_view: Arc<ImageView>,
    pub(super) sampler: Arc<Sampler>,
}

impl Texture {
    pub(super) fn new<P: AsRef<std::path::Path>>(
        vulkan_context: &vulkan_context::VulkanContext,
        usage: ImageUsage,
        path: P,
        name: Option<&str>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        tracing::debug!("Texture::New");

        // Read png
        let decoder = png::Decoder::new(BufReader::new(std::fs::File::open(path)?));
        let reader = decoder.read_info()?;

        let buf = vec![0u8; reader.output_buffer_size().unwrap()];
        tracing::debug!("image reading done");
        let info = reader.info();
        let buf_size = size_of_val(buf.as_slice());
        tracing::debug!("buffersize is {}", buf_size);
        tracing::debug!("bufferlen is {}", buf.len());

        let image = vulkano::image::Image::new(
            &vulkan_context.allocator, 
            &ImageCreateInfo{
                initial_layout: ImageLayout::Undefined,
                usage: usage | ImageUsage::TRANSFER_DST,
                image_type: ImageType::Dim2d,
                format: Format::R8G8B8A8_SRGB,
                extent: [info.width, info.height, 1],
                tiling: ImageTiling::Optimal,
                sharing: vulkano::sync::Sharing::Exclusive,
                samples: SampleCount::Sample1,
                mip_levels: 1,
                array_layers: 1,
                ..Default::default()
            }, 
            &AllocationCreateInfo::default())?;

        let image_view = ImageView::new(
            &image,
            &ImageViewCreateInfo{
                view_type: ImageViewType::Dim2d,
                format: Format::R8G8B8A8_SRGB,
                subresource_range: ImageSubresourceRange{
                    aspects: ImageAspects::COLOR,
                    base_mip_level: 0,
                    level_count: Some(1),
                    base_array_layer: 0,
                    layer_count: Some(1)
                },
                ..Default::default()
            }
        )?;

        if let Some(name) = name {
            unsafe {
                image.set_debug_utils_object_name(Some(name))?;
                image.set_debug_utils_object_name(Some(format!("{} view", name).as_str()))?;
            }
        }

        // Copy image data to buffer (CPU)
        let buffer = Buffer::new_slice(
            &vulkan_context.allocator,
            &BufferCreateInfo{
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            &AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            buf_size as u64
        )?;

		// Copy image data
        {
            tracing::debug!("copying image data to buffer");
            let mut image_data = buffer.write().unwrap();
            image_data.copy_from_slice(&buf);
            //image_data = &mut image_data[(info.width * info.height * 4) as usize..];

            //data.copy_from(, buf_size as _);
            tracing::debug!("done");
        }

        // Copy buffer to image (GPU)
        let mut command_buffer = AutoCommandBufferBuilder::primary(
            vulkan_context.command_buffer_allocator.clone(),
            vulkan_context.graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit
        )?;

        command_buffer
            .copy_buffer_to_image(CopyBufferToImageInfo::new(buffer, image.clone()))?;

        let _ = command_buffer
            .build()?
            .execute(vulkan_context.graphics_queue.clone())?;

        // Create sampler
        let sampler = Sampler::new(
            &vulkan_context.device,
            &SamplerCreateInfo {
                mag_filter: Filter::Linear,
                min_filter: Filter::Linear,
                address_mode: [SamplerAddressMode::Repeat; 3],
                anisotropy: Some(4.0),
                border_color: BorderColor::IntOpaqueBlack,
                unnormalized_coordinates: false,
                compare: None,
                mipmap_mode: SamplerMipmapMode::Linear,
                mip_lod_bias: 0.0,
                min_lod: 0.0,
                max_lod: Some(0.0),
                ..Default::default()
            },
        )?;

        Ok(Self {
            image,
            image_view,
            sampler,
        })
    }
}
