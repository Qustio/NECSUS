use std::{
    error::Error,
    ffi::{CStr, CString},
    fmt::format,
    ops::Deref,
    os::raw::c_void,
    sync::Arc,
};

use ash::{
    ext::device_address_binding_report,
    vk::{self, Offset3D},
};
use shipyard::Unique;
use vk_mem::Alloc;
use vulkano::image::{Image, sampler::Sampler, view::ImageView};

pub(super) struct AllocatedBuffer {
    allocator: Arc<vk_mem::Allocator>,
    pub(super) buffer: vk::Buffer,
    pub(super) allocation: vk_mem::Allocation,
    pub(super) info: vk_mem::AllocationInfo,
    name: Option<CString>,
}

impl std::fmt::Debug for AllocatedBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AllocatedBuffer")
            .field("buffer", &self.buffer)
            .field("allocation", &self.allocation)
            .field("info", &self.info)
            .finish()
    }
}

impl AllocatedBuffer {
    pub(super) fn new(
        allocator: &Arc<vk_mem::Allocator>,
        debug_device: ash::ext::debug_utils::Device,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        memory_usage: vk_mem::MemoryUsage,
        flags: vk_mem::AllocationCreateFlags,
        name: Option<&CStr>,
    ) -> Result<Self, Box<dyn Error>> {
        tracing::debug!("AllocatedBuffer::New");
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: memory_usage,
            flags: flags,
            ..Default::default()
        };

        let (buffer, allocation) = unsafe { allocator.create_buffer(&buffer_info, &alloc_info)? };
        let info = allocator.get_allocation_info(&allocation);

        let mut namestr = None;

        if let Some(name) = name {
            namestr = Some(name.to_owned());
            unsafe {
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_name(name)
                    .object_handle(buffer);
                debug_device.set_debug_utils_object_name(&name_info)?;
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_name(name)
                    .object_handle(info.device_memory);
                debug_device.set_debug_utils_object_name(&name_info)?;
            };
        }

        let allocator = allocator.clone();

        Ok(Self {
            allocator,
            buffer,
            allocation,
            info,
            name: namestr,
        })
    }
}

impl Drop for AllocatedBuffer {
    fn drop(&mut self) {
        unsafe {
            match &self.name {
                Some(name) => tracing::debug!("AllocatedBuffer::Drop[{:?}]", name),
                None => tracing::debug!("AllocatedBuffer::Drop"),
            }
            self.allocator
                .destroy_buffer(self.buffer, &mut self.allocation);
        }
    }
}

pub(super) struct AllocatedImage {
    allocator: Arc<vk_mem::Allocator>,
    pub(super) image: vk::Image,
    pub(super) allocation: vk_mem::Allocation,
    name: Option<CString>,
}

impl AllocatedImage {
    pub(super) fn new(
        allocator: &Arc<vk_mem::Allocator>,
        debug_device: &ash::ext::debug_utils::Device,
        layout: vk::ImageLayout,
        usage: vk::ImageUsageFlags,
        extent: vk::Extent3D,
        flags: vk_mem::AllocationCreateFlags,
        name: Option<&CStr>,
    ) -> Result<Self, Box<dyn Error>> {
        tracing::debug!("AllocatedImage::New");
        let allocator = allocator.clone();
        let image_info = vk::ImageCreateInfo::default()
            .initial_layout(layout)
            .usage(usage)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::D32_SFLOAT)
            .extent(extent)
            .tiling(vk::ImageTiling::OPTIMAL)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .mip_levels(1)
            .array_layers(1);

        let alloc_info = vk_mem::AllocationCreateInfo {
            flags: flags,
            ..Default::default()
        };

        let (image, allocation) =
            unsafe { allocator.create_image(&image_info, &alloc_info).unwrap() };

        let mut saved_name = None;

        if let Some(name) = name {
            let name = name.into();
            let image_name = CString::new(format!("{:?} image texture", name))?;
            unsafe {
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_name(image_name.as_c_str())
                    .object_handle(image);
                debug_device.set_debug_utils_object_name(&name_info)?;
            };

            saved_name = Some(name);
        }

        Ok(Self {
            allocator,
            image,
            allocation,
            name: saved_name,
        })
    }
}

impl Drop for AllocatedImage {
    fn drop(&mut self) {
        unsafe {
            match &self.name {
                Some(name) => tracing::debug!("AllocatedImage::Drop[{:?}]", name),
                None => tracing::debug!("AllocatedImage::Drop"),
            }
            self.allocator
                .destroy_image(self.image, &mut self.allocation);
        }
    }
}

#[derive(Unique)]
pub(super) struct AllocatedImageTexture {
    pub(super) image: Arc<Image>,
    pub(super) view: Arc<ImageView>,
    pub(super) sampler: Arc<Sampler>,
    pub(super) allocation: vk_mem::Allocation,
    name: Option<CString>,
}

impl AllocatedImageTexture {
    pub(super) fn new<P: AsRef<std::path::Path>>(
        device: &ash::Device,
        allocator: &Arc<vk_mem::Allocator>,
        debug_device: &ash::ext::debug_utils::Device,
        layout: vk::ImageLayout,
        usage: vk::ImageUsageFlags,
        flags: vk_mem::AllocationCreateFlags,
        graphics_queue: vk::Queue,
        command_pool: vk::CommandPool,
        path: P,
        name: Option<&CStr>,
    ) -> Result<Self, Box<dyn Error>> {
        tracing::debug!("AllocatedImageTexture::New");

        let decoder = png::Decoder::new(std::fs::File::open(path)?);
        let mut reader = decoder.read_info()?;

        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf)?;
        tracing::debug!("image reading done");
        let info = reader.info();
        let buf_size = size_of_val(buf.as_slice());
        tracing::debug!("buffersize is {}", buf_size);
        tracing::debug!("bufferlen is {}", buf.len());

        let extent = vk::Extent2D {
            width: info.width,
            height: info.height,
        };

        let allocator = allocator.clone();
        let image_info = vk::ImageCreateInfo::default()
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage | vk::ImageUsageFlags::TRANSFER_DST)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .extent(extent.into())
            .tiling(vk::ImageTiling::OPTIMAL)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .mip_levels(1)
            .array_layers(1);

        let alloc_info = vk_mem::AllocationCreateInfo {
            flags: flags,
            ..Default::default()
        };

        let (image, allocation) =
            unsafe { allocator.create_image(&image_info, &alloc_info).unwrap() };

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let view = unsafe { device.create_image_view(&view_info, None)? };

        let mut saved_name = None;

        if let Some(name) = name {
            let name = name.into();
            let image_name = CString::new(format!("{:?} image texture", name))?;
            let image_view_name = CString::new(format!("{:?} image view texture", name))?;
            unsafe {
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_name(image_name.as_c_str())
                    .object_handle(image);
                debug_device.set_debug_utils_object_name(&name_info)?;
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_name(image_view_name.as_c_str())
                    .object_handle(view);
                debug_device.set_debug_utils_object_name(&name_info)?;
            };
            saved_name = Some(name);
        }

        let s_buffer = AllocatedBuffer::new(
            &allocator,
            debug_device.clone(),
            buf_size as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk_mem::MemoryUsage::AutoPreferHost,
            vk_mem::AllocationCreateFlags::MAPPED
                | vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            Some(c"Staging Buffer"),
        )?;

        tracing::debug!("start copying");
        unsafe {
            // Copy vertex data
            let data = s_buffer.info.mapped_data as *mut u8;
            tracing::debug!("1");

            data.copy_from(buf.as_ptr(), buf_size as _);
            tracing::debug!("2");
        }

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(command_pool)
            .command_buffer_count(1);
        let command_buffers = unsafe { device.allocate_command_buffers(&alloc_info)? };

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        let imsbr = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);

        let image_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(image)
            .subresource_range(imsbr);

        let sbrs = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1)
            .mip_level(0);

        let region = vk::BufferImageCopy::default()
            .image_extent(extent.into())
            .image_offset(Offset3D::default())
            .image_subresource(sbrs)
            .buffer_row_length(0);

        let regions = [region];

        unsafe {
            device.begin_command_buffer(command_buffers[0], &begin_info)?;
            device.cmd_pipeline_barrier(
                command_buffers[0],
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier],
            );
            device.cmd_copy_buffer_to_image(
                command_buffers[0],
                s_buffer.buffer,
                image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                &regions,
            );
            device.end_command_buffer(command_buffers[0])?;
        }

        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

        let submits = [submit_info];

        unsafe {
            device.queue_submit(graphics_queue, &submits, vk::Fence::null())?;
            device.queue_wait_idle(graphics_queue)?;
            device.free_command_buffers(command_pool, &command_buffers);
        }

        // Create sampler
        let samlpler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            //todo physicaldeviceproperties.limits.maxsampleranisotropy
            .max_anisotropy(4.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            // Mipmapping
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = unsafe { device.create_sampler(&samlpler_info, None)? };

        let device = device.clone();

        Ok(Self {
            device,
            allocator,
            image,
            view,
            sampler,
            allocation,
            name: saved_name,
        })
    }
}

impl Drop for AllocatedImageTexture {
    fn drop(&mut self) {
        unsafe {
            match &self.name {
                Some(name) => tracing::debug!("AllocatedImage::Drop[{:?}]", name),
                None => tracing::debug!("AllocatedImage::Drop"),
            }
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_sampler(self.sampler, None);
            self.allocator
                .destroy_image(self.image, &mut self.allocation);
        }
    }
}
